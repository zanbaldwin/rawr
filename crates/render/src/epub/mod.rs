//! EPUB generation from AO3 HTML content.
//!
//! Converts extracted chapter content and metadata into a valid EPUB 2.0.1
//! archive (the most broadly compatible flavour across e-readers) with a
//! title page and automatic TOC generation. Requires the `epub` feature flag.
//!
//! # Usage
//!
//! ```no_run
//! use rawr_render::epub::{EpubRenderer, EpubInput};
//! use rawr_render::StyleConfig;
//! # use rawr_render::error::Result;
//! # use rawr_extract::models::{Metadata, ChapterContent};
//!
//! # fn example(metadata: Metadata, chapters: Vec<ChapterContent>) -> Result<()> {
//! let renderer = EpubRenderer::new(
//!     StyleConfig::new().with_builtin("epub.css")?
//! );
//!
//! let input = EpubInput { metadata, chapters };
//! let output = renderer.render(&input)?;
//! println!("EPUB at: {}", output.path().display());
//! # Ok(())
//! # }
//! ```

use crate::error::{ErrorKind, Result};
use crate::style::StyleConfig;
use epub_builder::{EpubBuilder, EpubContent, EpubVersion, ReferenceType, ZipLibrary};
use exn::ResultExt;
use rawr_extract::models::{ChapterContent, Metadata};
use std::io::{Cursor, Write};
use std::path::PathBuf;
use tracing::debug;

pub const APP_NAME: &str = "rawr";

/// Input for EPUB generation: metadata and chapter content.
pub struct EpubInput {
    pub metadata: Metadata,
    pub chapters: Vec<ChapterContent>,
}

/// An AO3-to-EPUB renderer that produces EPUB 2.0.1 archives.
///
/// Construct via [`new()`](Self::new) with a [`StyleConfig`], then call
/// [`render()`](Self::render) or [`render_to()`](Self::render_to).
pub struct EpubRenderer {
    styles: StyleConfig,
}

impl EpubRenderer {
    pub fn new(styles: StyleConfig) -> Self {
        Self { styles }
    }

    /// Generate an EPUB to a temporary file that is deleted when dropped.
    pub fn render(&self, input: &EpubInput) -> Result<crate::TempFile> {
        let mut tmp = tempfile::Builder::new().suffix(".epub").tempfile().or_raise(|| ErrorKind::Io)?;
        self.write(input, &mut tmp)?;
        Ok(tmp)
    }

    /// Generate an EPUB to the specified path.
    pub fn render_to(&self, input: &EpubInput, path: impl Into<PathBuf>) -> Result<PathBuf> {
        let path = path.into();
        let mut file = std::fs::File::create(&path).or_raise(|| ErrorKind::Io)?;
        self.write(input, &mut file)?;
        Ok(path)
    }

    fn write(&self, input: &EpubInput, writer: &mut impl Write) -> Result<()> {
        if input.chapters.is_empty() {
            exn::bail!(ErrorKind::NoChapterContent);
        }
        let mut builder = EpubBuilder::new(ZipLibrary::new().or_raise(|| ErrorKind::EpubGeneration)?)
            .or_raise(|| ErrorKind::EpubGeneration)?;
        builder.epub_version(EpubVersion::V20);
        self.set_metadata(&mut builder, &input.metadata)?;
        self.add_stylesheet(&mut builder)?;
        self.add_title_page(&mut builder, &input.metadata)?;
        if input.chapters.len() > 1 {
            builder.set_toc_name("Contents");
            builder.inline_toc();
        }
        self.add_chapters(&mut builder, &input.metadata, &input.chapters)?;
        builder.generate(writer).or_raise(|| ErrorKind::EpubGeneration)?;
        debug!(work_id = input.metadata.work_id, chapters = input.chapters.len(), "EPUB generated");
        Ok(())
    }

    fn set_metadata(&self, builder: &mut EpubBuilder<ZipLibrary>, meta: &Metadata) -> Result<()> {
        builder.metadata("title", &meta.title).or_raise(|| ErrorKind::EpubGeneration)?;
        // TODO: Is lang a required piece of epub metadata? Can we omit it if iso_code is none?
        builder.metadata("lang", meta.language.iso_code.unwrap_or("en")).or_raise(|| ErrorKind::EpubGeneration)?;
        builder.metadata("generator", APP_NAME).or_raise(|| ErrorKind::EpubGeneration)?;
        for author in &meta.authors {
            builder.metadata("author", author.to_string()).or_raise(|| ErrorKind::EpubGeneration)?;
        }
        if let Some(summary) = &meta.summary {
            builder.metadata("description", summary.as_str()).or_raise(|| ErrorKind::EpubGeneration)?;
        }
        for fandom in &meta.fandoms {
            builder.metadata("subject", fandom.to_string()).or_raise(|| ErrorKind::EpubGeneration)?;
        }
        for tag in &meta.tags {
            builder.metadata("subject", &tag.name).or_raise(|| ErrorKind::EpubGeneration)?;
        }
        Ok(())
    }

    fn add_stylesheet(&self, builder: &mut EpubBuilder<ZipLibrary>) -> Result<()> {
        let mut css = Vec::new();
        self.styles.write_raw_to(&mut css).or_raise(|| ErrorKind::Io)?;
        if !css.is_empty() {
            builder.stylesheet(Cursor::new(css)).or_raise(|| ErrorKind::EpubGeneration)?;
        }
        Ok(())
    }

    fn add_chapters(
        &self,
        builder: &mut EpubBuilder<ZipLibrary>,
        meta: &Metadata,
        chapters: &[ChapterContent],
    ) -> Result<()> {
        let multi_chapter = chapters.len() > 1;
        for chapter in chapters {
            let (display_title, xhtml) = chapter_document(meta, chapter, multi_chapter);
            let href = format!("chapter_{:04}.xhtml", chapter.number);
            let content = EpubContent::new(&href, Cursor::new(xhtml.into_bytes())).title(&display_title);
            builder.add_content(content).or_raise(|| ErrorKind::EpubGeneration)?;
        }
        Ok(())
    }

    fn add_title_page(&self, builder: &mut EpubBuilder<ZipLibrary>, meta: &Metadata) -> Result<()> {
        let body = title_page_body(meta);
        let xhtml = wrap_xhtml_body(&body, &meta.title, Some("stylesheet.css"), meta.language.iso_code.unwrap_or("en"));
        let content = EpubContent::new("title_page.xhtml", Cursor::new(xhtml.into_bytes()))
            .title("Title Page")
            .reftype(ReferenceType::TitlePage);
        builder.add_content(content).or_raise(|| ErrorKind::EpubGeneration)?;
        Ok(())
    }
}

/// Build one chapter's display title and complete XHTML document.
///
/// Multi-chapter works get a `Chapter N[: Title]` heading at the top of the
/// body (no heading otherwise exists in AO3 chapter content); one-shots get
/// no heading and take the work title for reader navigation.
fn chapter_document(meta: &Metadata, chapter: &ChapterContent, multi_chapter: bool) -> (String, String) {
    let lang = meta.language.iso_code.unwrap_or("en");
    let (display_title, body) = if multi_chapter {
        // AO3 chapter headings usually already carry a "Chapter N" prefix;
        // only prepend one when the extracted title lacks it.
        let prefix = format!("Chapter {}", chapter.number);
        let display_title = match chapter.title.as_deref() {
            Some(title)
                if title.strip_prefix(&prefix).is_some_and(|rest| rest.is_empty() || rest.starts_with([':', ' '])) =>
            {
                title.to_string()
            },
            Some(title) => format!("{prefix}: {title}"),
            None => prefix,
        };
        let heading = format!("<h2 class=\"chapter-title\">{}</h2>\n", xml_escape(&display_title));
        (display_title, format!("{heading}{}", chapter.body_html))
    } else {
        (meta.title.clone(), chapter.body_html.clone())
    };
    let xhtml = wrap_xhtml_body(&body, &display_title, Some("stylesheet.css"), lang);
    (display_title, xhtml)
}

/// Build the title page body: work title, byline, summary and a small
/// metadata list. Warnings and the full tag list intentionally stay in the
/// OPF metadata only.
fn title_page_body(meta: &Metadata) -> String {
    let mut body = String::from("<div class=\"title-page\">\n");
    body.push_str(&format!("<h1 class=\"work-title\">{}</h1>\n", xml_escape(&meta.title)));
    let byline = if meta.authors.is_empty() {
        "Anonymous".to_string()
    } else {
        meta.authors.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
    };
    body.push_str(&format!("<p class=\"byline\">by {}</p>\n", xml_escape(&byline)));
    if let Some(summary) = &meta.summary {
        body.push_str("<div class=\"summary\">\n");
        for paragraph in summary.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
            body.push_str(&format!("<p>{}</p>\n", xml_escape(paragraph)));
        }
        body.push_str("</div>\n");
    }
    body.push_str("<dl class=\"work-meta\">\n");
    let mut item = |term: &str, detail: String| {
        body.push_str(&format!("<dt>{term}</dt><dd>{}</dd>\n", xml_escape(&detail)));
    };
    if !meta.fandoms.is_empty() {
        item("Fandom", meta.fandoms.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "));
    }
    for series in &meta.series {
        item("Series", series.to_string());
    }
    if let Some(rating) = &meta.rating {
        item("Rating", rating.to_string());
    }
    item("Words", meta.words.to_string());
    if meta.chapters.written > 1 {
        item("Chapters", meta.chapters.to_string());
    }
    item("Published", meta.published.to_string());
    if meta.last_modified != meta.published {
        item("Updated", meta.last_modified.to_string());
    }
    body.push_str("</dl>\n</div>");
    body
}

/// Wrap an XHTML body fragment in a complete XHTML document.
///
/// The `body_xhtml` parameter must already be valid XHTML content
/// (e.g., from [`chapters_xhtml()`](rawr_extract::Extractor::chapters_xhtml)).
pub(crate) fn wrap_xhtml_body(body_xhtml: &str, title: &str, css_href: Option<&str>, lang: &str) -> String {
    let css_link =
        css_href.map(|href| format!(r#"<link rel="stylesheet" type="text/css" href="{href}"/>"#)).unwrap_or_default();
    let escaped_title = xml_escape(title);
    let escaped_lang = xml_escape(lang);
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="{escaped_lang}">
<head>
<title>{escaped_title}</title>
{css_link}
</head>
<body>
{body_xhtml}
</body>
</html>"#
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawr_extract::models::{Author, ChapterCount, Fandom, Language, Metadata, Rating, SeriesPosition};
    use std::str::FromStr;
    use time::{Date, Month};

    fn meta() -> Metadata {
        Metadata {
            work_id: 12345,
            title: "Tea & Biscuits".to_string(),
            authors: vec![Author::new("zan", None::<&str>)],
            fandoms: vec![Fandom { name: "Testing".to_string() }],
            series: vec![],
            chapters: ChapterCount { written: 2, total: Some(2) },
            words: 1000,
            rating: Some(Rating::GeneralAudiences),
            warnings: vec![],
            tags: vec![],
            summary: None,
            language: Language::from_str("English").unwrap(),
            published: Date::from_calendar_date(2024, Month::January, 1).unwrap(),
            last_modified: Date::from_calendar_date(2024, Month::January, 1).unwrap(),
        }
    }

    fn chapter(number: u32, title: Option<&str>) -> ChapterContent {
        ChapterContent {
            number,
            title: title.map(ToString::to_string),
            summary: None,
            body_html: "<p class=\"first\">Once upon a time.</p>".to_string(),
            author_notes: None,
            end_notes: None,
        }
    }

    #[test]
    fn wrapper_emits_xhtml_11_doctype_and_lang() {
        let doc = wrap_xhtml_body("<p>x</p>", "T", Some("stylesheet.css"), "en");
        assert!(doc.contains(r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN""#));
        assert!(doc.contains(r#"<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">"#));
        assert!(doc.contains(r#"<link rel="stylesheet" type="text/css" href="stylesheet.css"/>"#));
    }

    #[test]
    fn multi_chapter_gets_heading_with_title() {
        let (display, xhtml) = chapter_document(&meta(), &chapter(1, Some("The Kettle")), true);
        assert_eq!(display, "Chapter 1: The Kettle");
        assert!(xhtml.contains(r#"<h2 class="chapter-title">Chapter 1: The Kettle</h2>"#));
    }

    #[test]
    fn multi_chapter_prefixed_title_not_duplicated() {
        let (display, _) = chapter_document(&meta(), &chapter(1, Some("Chapter 1: Where Woes Begin")), true);
        assert_eq!(display, "Chapter 1: Where Woes Begin");
        let (display, _) = chapter_document(&meta(), &chapter(3, Some("Chapter 3")), true);
        assert_eq!(display, "Chapter 3");
        // A different chapter's number is not a prefix match.
        let (display, _) = chapter_document(&meta(), &chapter(1, Some("Chapter 12 Revisited")), true);
        assert_eq!(display, "Chapter 1: Chapter 12 Revisited");
    }

    #[test]
    fn multi_chapter_untitled_gets_numbered_heading() {
        let (display, xhtml) = chapter_document(&meta(), &chapter(2, None), true);
        assert_eq!(display, "Chapter 2");
        assert!(xhtml.contains(r#"<h2 class="chapter-title">Chapter 2</h2>"#));
    }

    #[test]
    fn heading_escapes_markup_in_title() {
        let (_, xhtml) = chapter_document(&meta(), &chapter(1, Some("Salt & <Pepper>")), true);
        assert!(xhtml.contains(r#"<h2 class="chapter-title">Chapter 1: Salt &amp; &lt;Pepper&gt;</h2>"#));
    }

    #[test]
    fn single_chapter_gets_no_heading_and_work_title() {
        let (display, xhtml) = chapter_document(&meta(), &chapter(1, Some("Ignored")), false);
        assert_eq!(display, "Tea & Biscuits");
        assert!(!xhtml.contains("chapter-title"));
        assert!(xhtml.contains("<title>Tea &amp; Biscuits</title>"));
    }

    #[test]
    fn title_page_basic_fields() {
        let body = title_page_body(&meta());
        assert!(body.contains(r#"<h1 class="work-title">Tea &amp; Biscuits</h1>"#));
        assert!(body.contains(r#"<p class="byline">by zan</p>"#));
        assert!(body.contains("<dt>Fandom</dt><dd>Testing</dd>"));
        assert!(body.contains("<dt>Rating</dt><dd>General Audiences</dd>"));
        assert!(body.contains("<dt>Words</dt><dd>1000</dd>"));
        assert!(body.contains("<dt>Chapters</dt><dd>2/2</dd>"));
        assert!(body.contains("<dt>Published</dt><dd>2024-01-01</dd>"));
    }

    #[test]
    fn title_page_anonymous_when_no_authors() {
        let body = title_page_body(&Metadata { authors: vec![], ..meta() });
        assert!(body.contains(r#"<p class="byline">by Anonymous</p>"#));
    }

    #[test]
    fn title_page_omits_missing_summary_and_rating() {
        let body = title_page_body(&Metadata { rating: None, ..meta() });
        assert!(!body.contains("summary"));
        assert!(!body.contains("<dt>Rating</dt>"));
    }

    #[test]
    fn title_page_summary_split_into_paragraphs() {
        let body = title_page_body(&Metadata {
            summary: Some("First paragraph.\n\nSecond & final.".to_string()),
            ..meta()
        });
        assert!(body.contains("<div class=\"summary\">\n<p>First paragraph.</p>\n<p>Second &amp; final.</p>\n</div>"));
    }

    #[test]
    fn title_page_updated_suppressed_when_unchanged() {
        let body = title_page_body(&meta());
        assert!(!body.contains("<dt>Updated</dt>"));
        let updated = Metadata {
            last_modified: Date::from_calendar_date(2024, Month::June, 15).unwrap(),
            ..meta()
        };
        assert!(title_page_body(&updated).contains("<dt>Updated</dt><dd>2024-06-15</dd>"));
    }

    #[test]
    fn title_page_chapters_suppressed_for_single_chapter() {
        let single = Metadata {
            chapters: ChapterCount { written: 1, total: Some(1) },
            ..meta()
        };
        assert!(!title_page_body(&single).contains("<dt>Chapters</dt>"));
    }

    #[test]
    fn title_page_series_listed() {
        let with_series = Metadata {
            series: vec![SeriesPosition::new(9, "The Pantry", 3)],
            ..meta()
        };
        assert!(title_page_body(&with_series).contains(r#"<dt>Series</dt><dd>Part 3 of &quot;The Pantry&quot;</dd>"#));
    }
}
