//! Main extraction logic for AO3 HTML downloads.

mod data;
mod stats;

use std::convert::Infallible;
use std::str::FromStr;

pub use self::data::Datalist;
pub use self::stats::Stats;
use crate::error::{Error, ErrorKind, Result};
use crate::models::{Author, Metadata};
use crate::{ESTIMATED_HEADER_SIZE_BYTES, consts, safe_html_truncate};
use exn::{OptionExt, ResultExt};
use html2md::rewrite_html as html_to_markdown;
use scraper::Html;
use tracing::instrument;

#[derive(Debug)]
pub struct Extractor {
    document: Html,
}
impl Extractor {
    pub fn from_document(document: Html) -> Self {
        Self { document }
    }

    pub fn from_html(html: &str) -> Self {
        let document = Html::parse_document(html);
        Self::from_document(document)
    }

    /// Construct an [`Extractor`] from a large HTML document; it will
    /// automatically truncate the HTML to an appropriate size.
    pub fn from_long_html(html: &str) -> Self {
        Self::from_html(safe_html_truncate(html, ESTIMATED_HEADER_SIZE_BYTES))
    }

    /// Extraction of the metadata automatically performs a validity check,
    /// so [`is_valid`](Self::is_valid) is only useful if you don't plan on
    /// extracting metadata.
    pub fn is_valid(&self) -> bool {
        self.work_id().is_ok()
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
    #[instrument()]
    pub fn metadata(self) -> Result<Metadata> {
        // Always attempt extraction of the Work ID first, it's
        // equivalent to quickly checking the HTML document validity.
        let work_id = self.work_id().or_raise(|| ErrorKind::InvalidDocument)?;
        let datalist = self.datalist();
        let stats = datalist.stats()?;
        let (published, last_modified) = stats.dates()?;
        Ok(Metadata {
            // Main Document
            work_id,
            title: self.title().or_raise(|| ErrorKind::MissingField("title"))?,
            authors: self.authors(),
            summary: self.summary(),
            // Datalist
            fandoms: datalist.fandoms(),
            series: datalist.series(),
            rating: datalist.rating().or_raise(|| ErrorKind::MissingField("rating"))?,
            warnings: datalist.warnings(),
            tags: datalist.tags(),
            language: datalist.language(),
            // Datalist -> Stats
            chapters: stats.chapters()?,
            words: stats.words()?,
            published,
            last_modified,
        })
    }

    fn work_id(&self) -> Result<u64> {
        for element in self.document.select(&consts::WORK_URL_SELECTOR) {
            if let Some(href) = element.value().attr("href")
                && let Some(captures) = consts::WORK_URL_REGEX.captures(href)
                && let Some(id_str) = captures.get(1)
            {
                return id_str.as_str().parse::<u64>().or_raise(|| ErrorKind::ParseError {
                    field: "work_id",
                    value: id_str.as_str().to_string(),
                });
            }
        }
        exn::bail!(ErrorKind::MissingField("id"));
    }

    fn title(&self) -> Result<String> {
        self.document
            .select(&consts::TITLE_SELECTOR)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_raise(|| ErrorKind::MissingField("title"))
    }

    fn authors(&self) -> Vec<Author> {
        let mut authors = Vec::new();
        for element in self.document.select(&consts::BYLINE_SELECTOR) {
            if let Some(href) = element.value().attr("href")
                && let Some(captures) = consts::AUTHOR_REGEX.captures(href)
                && let Some(username) = captures.get(1).map(|m| m.as_str().to_string())
            {
                let pseudonym = captures.get(2).map(|m| m.as_str().to_string());
                let author: Author = (username, pseudonym).into();
                // Filter out `orphan_account`, technically there are cases where the
                // account has been orphaned but the pseudonym hasn't, but... I don't
                // want to deal with that headache.
                if author.username != "orphan_account" {
                    authors.push(author);
                }
            }
        }
        authors.sort();
        authors.dedup();
        authors
    }

    fn summary(&self) -> String {
        self.document
            .select(&consts::SUMMARY_SELECTOR)
            .next()
            .map(|el| html_to_markdown(el.inner_html().as_str(), true))
            .unwrap_or_default()
    }

    fn datalist(&self) -> Datalist<'_> {
        data::Datalist::new(&self.document)
    }
}
impl FromStr for Extractor {
    type Err = Infallible;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self::from_long_html(s))
    }
}
impl From<String> for Extractor {
    fn from(value: String) -> Self {
        Self::from_long_html(&value)
    }
}
impl From<Html> for Extractor {
    fn from(document: Html) -> Self {
        Self::from_document(document)
    }
}

impl TryFrom<Extractor> for Metadata {
    type Error = Error;
    fn try_from(extractor: Extractor) -> Result<Self> {
        extractor.metadata()
    }
}

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
    Extractor::from_long_html(html).is_valid()
}
