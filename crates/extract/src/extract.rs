//! Main extraction logic for AO3 HTML downloads.

use std::collections::HashSet;

use ::regex::{Regex, escape as regex_escape};
use exn::{OptionExt, ResultExt};
use html2md::rewrite_html as html_to_markdown;
use scraper::{ElementRef, Html};
use time::{Date, Month};
use tracing::instrument;

use crate::error::{ErrorKind, Result};
use crate::models::{Author, Chapters, Fandom, Language, Metadata, Rating, SeriesPosition, Tag, TagKind, Warning};
use crate::{ESTIMATED_HEADER_SIZE_BYTES, consts, safe_html_truncate};

/// Returns `true` if the HTML content appears to be a valid AO3 work download.
///
/// # Validation criteria
/// > Contains a valid AO3 work URL in `div#preface p.message a`
///
/// This function is designed to be fast and only examines the necessary parts
/// of the document.
///
/// # Examples
///
/// ```rust
/// use rawr_extract::is_valid;
/// let valid_html = r#"
///     <div id="preface">
///         <p class="message">
///             <a href="https://archiveofourown.org/works/12345">Work Link</a>
///         </p>
///     </div>
/// "#;
///
/// assert!(is_valid(valid_html));
/// ```
#[instrument(skip(html), fields(html_size = html.len()))]
pub fn is_valid(html: &str) -> bool {
    let html = safe_html_truncate(html, ESTIMATED_HEADER_SIZE_BYTES);
    let document = Html::parse_document(html);
    self::work_id(&document).is_ok()
}

/// Extracts work metadata from AO3 HTML.
///
/// Returns a `Metadata` struct containing all extracted fields. The caller
/// is responsible for combining this with file-level data (`content_hash`,
/// `file_size`) to create a full `Version`.
///
/// # Errors
///
/// Returns an error if:
/// - The HTML is not a valid AO3 download
/// - Required fields cannot be found or parsed
#[instrument(skip(html), fields(html_size = html.len(), work_id))]
pub fn extract(html: &str) -> Result<Metadata> {
    let document = Html::parse_document(html);
    let tags = self::find_tags(&document);
    let stats = self::stats(tags.as_ref());
    let chapters = self::chapters(stats.as_deref())?;
    let (published, last_modified) = dates(stats.as_deref())?;
    let metadata = Metadata {
        work_id: self::work_id(&document).or_raise(|| ErrorKind::InvalidDocument)?,
        title: self::title(&document)?,
        authors: self::authors(&document),
        fandoms: self::fandoms(tags.as_ref()),
        rating: self::rating(tags.as_ref())?,
        warnings: self::warnings(tags.as_ref()),
        tags: self::tags(tags.as_ref()),
        summary: self::summary(&document),
        language: self::language(tags.as_ref()),
        chapters,
        words: self::words(stats.as_deref())?,
        published,
        last_modified,
        series: self::series(tags.as_ref()),
    };
    tracing::Span::current().record("work_id", metadata.work_id);
    Ok(metadata)
}

/// Extracts the work ID from the preface message link.
#[instrument(level = "trace")]
pub(crate) fn work_id(document: &Html) -> Result<u64> {
    for element in document.select(&consts::WORK_URL_SELECTOR) {
        if let Some(href) = element.value().attr("href")
            && let Some(captures) = consts::WORK_URL_REGEX.captures(href)
            && let Some(id_str) = captures.get(1)
        {
            return id_str.as_str().parse::<u64>().or_raise(|| ErrorKind::ParseError {
                field: "work_id",
                value: "not a number".to_string(),
            });
        }
    }
    exn::bail!(ErrorKind::MissingField("id"));
}

/// Extracts the work title.
#[instrument(level = "trace")]
pub(crate) fn title(document: &Html) -> Result<String> {
    document
        .select(&consts::TITLE_SELECTOR)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_raise(|| ErrorKind::MissingField("title"))
}

/// Extracts authors from byline links.
#[instrument(level = "trace")]
pub(crate) fn authors(document: &Html) -> Vec<Author> {
    let mut authors = Vec::new();
    for element in document.select(&consts::BYLINE_SELECTOR) {
        if let Some(href) = element.value().attr("href")
            && let Some(captures) = consts::AUTHOR_REGEX.captures(href)
            && let Some(username) = captures.get(1).map(|m| m.as_str().to_string())
        {
            let pseudonym = captures.get(2).map(|m| m.as_str().to_string());
            let author: Author = (username, pseudonym).into();
            // Filter out `orphan_account``, technically there are cases where the
            // account has been orphaned but the pseudonym hasn't, but... I don't
            // want to deal with that headache.
            if author.username != "orphan_account" && !authors.contains(&author) {
                authors.push(author);
            }
        }
    }
    authors
}

/// Finds the tags definition list in the preface.
pub(crate) fn find_tags(document: &Html) -> Option<ElementRef<'_>> {
    document.select(&consts::TAGS_DL_SELECTOR).next()
}

/// Finds a `dd` element following a `dt` with the given label(s).
fn find_dd_by_label<'a>(tags_dl: Option<&ElementRef<'a>>, labels: &[&str]) -> Option<ElementRef<'a>> {
    let tags_dl = tags_dl?;
    let dts: Vec<_> = tags_dl.select(&consts::DT_SELECTOR).collect();
    let dds: Vec<_> = tags_dl.select(&consts::DD_SELECTOR).collect();
    for (i, dt) in dts.iter().enumerate() {
        let dt_text = dt.text().collect::<String>();
        let dt_text = dt_text.trim().trim_end_matches(':');
        for label in labels {
            if dt_text.eq_ignore_ascii_case(label)
                && let Some(dd) = dds.get(i)
            {
                return Some(*dd);
            }
        }
    }
    None
}

/// Extracts text content from a `dd` element by label(s).
fn extract_dd_text(tags_dl: Option<&ElementRef>, labels: &[&str]) -> Option<String> {
    find_dd_by_label(tags_dl, labels).map(|dd| dd.text().collect::<String>().trim().to_string())
}

/// Extracts link texts from a `dd` element by label(s).
fn extract_dd_link_texts(tags_dl: Option<&ElementRef>, labels: &[&str]) -> Vec<String> {
    let Some(dd) = find_dd_by_label(tags_dl, labels) else {
        return Vec::new();
    };
    let mut texts = Vec::new();
    let mut seen = HashSet::new();
    for anchor in dd.select(&consts::ANCHOR_SELECTOR) {
        let text = anchor.text().collect::<String>().trim().to_string();
        if !text.is_empty() && !seen.contains(&text) {
            seen.insert(text.clone());
            texts.push(text);
        }
    }
    texts
}

/// Extracts fandoms.
#[instrument(level = "trace")]
pub(crate) fn fandoms(tags_dl: Option<&ElementRef>) -> Vec<Fandom> {
    extract_dd_link_texts(tags_dl, &["Fandom", "Fandoms"]).into_iter().map(|name| Fandom { name }).collect()
}

/// Extracts and maps the rating.
#[instrument(level = "trace")]
pub(crate) fn rating(tags_dl: Option<&ElementRef>) -> Result<Rating> {
    let text = extract_dd_text(tags_dl, &["Rating"]).ok_or_raise(|| ErrorKind::MissingField("rating"))?;
    Ok(match text.as_str() {
        "General Audiences" => Rating::GeneralAudiences,
        "Teen And Up Audiences" => Rating::TeenAndUp,
        "Mature" => Rating::Mature,
        "Explicit" => Rating::Explicit,
        "Not Rated" => Rating::NotRated,
        _ => exn::bail!(ErrorKind::ParseError {
            field: "rating",
            value: format!("unknown rating: {}", text),
        }),
    })
}

/// Extracts and maps warnings.
#[instrument(level = "trace")]
pub(crate) fn warnings(tags_dl: Option<&ElementRef>) -> Vec<Warning> {
    extract_dd_link_texts(tags_dl, &["Warning", "Warnings", "Archive Warning", "Archive Warnings"])
        .into_iter()
        .filter_map(|text| match text.as_str() {
            "No Archive Warnings Apply" => Some(Warning::NoWarningsApply),
            "Creator Chose Not To Use Archive Warnings" => Some(Warning::CreatorChoseNotToUse),
            "Graphic Depictions Of Violence" => Some(Warning::GraphicViolence),
            "Major Character Death" => Some(Warning::MajorCharacterDeath),
            "Underage" => Some(Warning::Underage),
            "Rape/Non-Con" => Some(Warning::NonCon),
            _ => None,
        })
        .collect()
}

/// Extracts all tags (relationships, characters, freeform).
#[instrument(level = "trace")]
pub(crate) fn tags(tags_dl: Option<&ElementRef>) -> Vec<Tag> {
    let mut tags = Vec::new();
    // Relationships
    for name in extract_dd_link_texts(tags_dl, &["Relationship", "Relationships"]) {
        tags.push(Tag { name, kind: TagKind::Relationship });
    }
    // Characters
    for name in extract_dd_link_texts(tags_dl, &["Character", "Characters"]) {
        tags.push(Tag { name, kind: TagKind::Character });
    }
    // Freeform/Additional tags
    for name in extract_dd_link_texts(tags_dl, &["Additional Tag", "Additional Tags"]) {
        tags.push(Tag { name, kind: TagKind::Freeform });
    }
    tags
}

/// Extracts and maps language.
#[instrument(level = "trace")]
pub(crate) fn language(tags_dl: Option<&ElementRef>) -> Language {
    Language::from(extract_dd_text(tags_dl, &["Language"]).unwrap_or_else(|| "Unknown".to_string()))
}

/// Extracts the summary and converts to Markdown.
#[instrument(level = "trace")]
pub(crate) fn summary(document: &Html) -> String {
    document
        .select(&consts::SUMMARY_SELECTOR)
        .next()
        .map(|el| html_to_markdown(el.inner_html().as_str(), true))
        .unwrap_or_default()
}

/// Extracts the stats text block.
pub(crate) fn stats(tags_dl: Option<&ElementRef>) -> Option<String> {
    extract_dd_text(tags_dl, &["Stats"])
}

/// Extracts chapter information from stats.
#[instrument(level = "trace")]
pub(crate) fn chapters(stats: Option<&str>) -> Result<Chapters> {
    let stats = stats.ok_or_raise(|| ErrorKind::MissingField("chapters"))?;
    let captures = consts::CHAPTERS_REGEX.captures(stats).ok_or_raise(|| ErrorKind::MissingField("chapters"))?;
    let current_str = captures.get(1).unwrap().as_str().replace(',', "");
    let current: u32 = current_str.parse::<u32>().or_raise(|| ErrorKind::ParseError {
        field: "chapters",
        value: "invalid chapter count".to_string(),
    })?;
    let total_str = captures.get(2).unwrap().as_str();
    let total = if total_str == "?" {
        None
    } else {
        let total_clean = total_str.replace(',', "");
        Some(total_clean.parse::<u32>().or_raise(|| ErrorKind::ParseError {
            field: "chapters",
            value: "invalid total chapters".to_string(),
        })?)
    };
    Ok(Chapters { written: current, total })
}

/// Extracts word count from stats.
#[instrument(level = "trace")]
pub(crate) fn words(stats: Option<&str>) -> Result<u64> {
    let stats = stats.ok_or_raise(|| ErrorKind::MissingField("word_count"))?;
    let captures = consts::WORDS_REGEX.captures(stats).ok_or_raise(|| ErrorKind::MissingField("word_count"))?;
    let word_str = captures.get(1).unwrap().as_str().replace(',', "");
    word_str.parse::<u64>().or_raise(|| ErrorKind::ParseError {
        field: "word_count",
        value: "invalid word count".to_string(),
    })
}

/// Extracts dates (published, last_modified) from stats.
#[instrument(level = "trace")]
pub(crate) fn dates(stats: Option<&str>) -> Result<(Date, Date)> {
    let stats = stats.ok_or_raise(|| ErrorKind::MissingField("published"))?;
    let mut published: Option<Date> = None;
    let mut last_modified: Option<Date> = None;
    for captures in consts::DATE_REGEX.captures_iter(stats) {
        let year: i32 = captures.get(2).unwrap().as_str().parse::<i32>().or_raise(|| ErrorKind::ParseError {
            field: "date-year",
            value: "invalid year number".to_string(),
        })?;
        let month: u8 =
            captures.get(3).unwrap().as_str().parse::<u8>().ok().filter(|m| (1..=12).contains(m)).ok_or_raise(
                || ErrorKind::ParseError {
                    field: "date-month",
                    value: "invalid month number".to_string(),
                },
            )?;
        let day: u8 =
            captures.get(4).unwrap().as_str().parse::<u8>().ok().filter(|d| (1..=31).contains(d)).ok_or_raise(
                || ErrorKind::ParseError {
                    field: "date-day",
                    value: "invalid date number".to_string(),
                },
            )?;
        let month = Month::try_from(month).or_raise(|| ErrorKind::ParseError {
            field: "date",
            value: "invalid month".to_string(),
        })?;
        let date = Date::from_calendar_date(year, month, day).or_raise(|| ErrorKind::ParseError {
            field: "date",
            value: "invalid date".to_string(),
        })?;
        match captures.get(1).unwrap().as_str() {
            "Published" => published = Some(date),
            "Updated" | "Completed" => last_modified = Some(date),
            _ => {},
        }
    }
    let published = published.ok_or_raise(|| ErrorKind::MissingField("published"))?;
    // For single-chapter works that were never updated, last_modified equals published
    let last_modified = last_modified.unwrap_or(published);
    Ok((published, last_modified))
}

/// Extracts series positions.
#[instrument(level = "trace")]
pub(crate) fn series(tags_dl: Option<&ElementRef>) -> Vec<SeriesPosition> {
    let Some(dd) = find_dd_by_label(tags_dl, &["Series"]) else {
        return Vec::new();
    };
    let dd_text = dd.text().collect::<String>();
    let mut series = Vec::new();
    let mut seen_ids = HashSet::new();
    for anchor in dd.select(&consts::ANCHOR_SELECTOR) {
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Some(captures) = consts::SERIES_URL_REGEX.captures(href) else {
            continue;
        };
        let series_id: u64 = match captures.get(1).unwrap().as_str().parse() {
            Ok(id) => id,
            Err(_) => continue,
        };
        // Deduplicate
        if seen_ids.contains(&series_id) {
            continue;
        }
        seen_ids.insert(series_id);
        let series_name = anchor.text().collect::<String>().trim().to_string();
        // Extract position: look for "Part N of {series_name}"
        // TODO: Can this be done via a lazy Regex?
        let position_pattern = format!(r"Part\s+(\d{{1,3}}(?:,?\d{{3}})*)\s+of\s+{}", regex_escape(&series_name));
        let position = Regex::new(&position_pattern)
            .ok()
            .and_then(|re| re.captures(&dd_text))
            .and_then(|cap| cap.get(1))
            .and_then(|m| m.as_str().replace(',', "").parse().ok())
            .unwrap_or(1);
        series.push(SeriesPosition {
            id: series_id,
            name: series_name,
            position,
        });
    }
    series
}
