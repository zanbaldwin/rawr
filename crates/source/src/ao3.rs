use crate::document::{Document, DocumentMeta, Origin, Section};
use crate::error::{ErrorKind, Result};
use crate::source::Source;
use exn::ResultExt;
use rawr_extract::Extractor;
use rawr_extract::models::{ChapterContent, Metadata};

const WORK_URL_PREFIX: &str = "https://archiveofourown.org/works/";

/// [`Source`] for AO3 HTML downloads.
///
/// Only the downloaded HTML format is supported: not AO3's other download
/// formats, the live site, or rawr's cache database.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ao3Html;

impl Source for Ao3Html {
    fn accepts(&self, input: &[u8]) -> bool {
        rawr_extract::is_valid(input)
    }

    fn to_document(&self, input: &[u8]) -> Result<Document> {
        if !self.accepts(input) {
            exn::bail!(ErrorKind::Unsupported);
        }
        let version = rawr_extract::extract(input).or_raise(|| ErrorKind::Extract)?;
        let sections: Vec<Section> = Extractor::from_html(input).chapters_xhtml().into_iter().map(section).collect();
        if sections.is_empty() {
            exn::bail!(ErrorKind::NoSections);
        }
        Ok(Document {
            meta: meta(&version.metadata),
            sections,
            origin: Origin::Ao3(Box::new(version)),
        })
    }
}

fn meta(metadata: &Metadata) -> DocumentMeta {
    DocumentMeta {
        identifier: format!("{WORK_URL_PREFIX}{}", metadata.work_id),
        title: metadata.title.clone(),
        authors: metadata.authors.iter().map(ToString::to_string).collect(),
        language: metadata.language.iso_code.map(String::from),
        summary: metadata.summary.clone(),
    }
}

fn section(chapter: ChapterContent) -> Section {
    Section {
        number: chapter.number,
        title: chapter.title,
        summary: chapter.summary,
        body_xhtml: chapter.body_html,
        notes: chapter.author_notes,
        end_notes: chapter.end_notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawr_extract::models::{Author, ChapterCount, Language, Rating};
    use rstest::rstest;
    use std::str::FromStr;
    use time::{Date, Month};

    const MINIMAL_WORK: &str = r#"<!DOCTYPE html>
<html><head><title>Tea &amp; Biscuits</title></head><body>
<div id="preface">
  <p class="message">Posted originally on the <a href="https://archiveofourown.org/">Archive of Our Own</a>
  at <a href="https://archiveofourown.org/works/12345">https://archiveofourown.org/works/12345</a>.</p>
  <div class="meta">
    <dl class="tags">
      <dt>Rating:</dt><dd><a href="https://archiveofourown.org/tags/General%20Audiences">General Audiences</a></dd>
      <dt>Language:</dt><dd>English</dd>
      <dt>Stats:</dt><dd>Published: 2024-01-01 Words: 4 Chapters: 1/1</dd>
    </dl>
    <h1>Tea &amp; Biscuits</h1>
    <div class="byline">by <a rel="author" href="https://archiveofourown.org/users/zan/pseuds/Zed">Zed (zan)</a></div>
    <p>Summary</p>
    <blockquote class="userstuff"><p>A short story.</p></blockquote>
  </div>
</div>
<div id="chapters"><div class="userstuff"><p>Once upon a time.</p></div></div>
</body></html>"#;

    fn metadata(language: &str) -> Metadata {
        Metadata {
            work_id: 12345,
            title: "Tea & Biscuits".to_string(),
            authors: vec![Author::new("zan", Some("Zed")), Author::new("kit", None::<&str>)],
            fandoms: vec![],
            series: vec![],
            chapters: ChapterCount { written: 1, total: Some(1) },
            words: 4,
            rating: Some(Rating::GeneralAudiences),
            warnings: vec![],
            tags: vec![],
            summary: Some("A short story.".to_string()),
            language: Language::from_str(language).unwrap(),
            published: Date::from_calendar_date(2024, Month::January, 1).unwrap(),
            last_modified: Date::from_calendar_date(2024, Month::January, 1).unwrap(),
        }
    }

    #[test]
    fn meta_maps_identifier_title_authors_summary() {
        let meta = meta(&metadata("English"));
        assert_eq!(meta.identifier, "https://archiveofourown.org/works/12345");
        assert_eq!(meta.title, "Tea & Biscuits");
        assert_eq!(meta.authors, vec!["Zed (zan)".to_string(), "kit".to_string()]);
        assert_eq!(meta.summary.as_deref(), Some("A short story."));
    }

    #[rstest]
    #[case("English", Some("en"))]
    #[case("한국어", Some("ko"))]
    #[case("Unknown", None)]
    fn meta_maps_language_to_iso_code(#[case] name: &str, #[case] expected: Option<&str>) {
        assert_eq!(meta(&metadata(name)).language.as_deref(), expected);
    }

    #[test]
    fn section_maps_every_chapter_field() {
        let chapter = ChapterContent {
            number: 3,
            title: Some("The Kettle".to_string()),
            summary: Some("<p>Summary</p>".to_string()),
            body_html: "<p class=\"first\">Body</p>".to_string(),
            author_notes: Some("<p>Before</p>".to_string()),
            end_notes: Some("<p>After</p>".to_string()),
        };
        assert_eq!(
            section(chapter),
            Section {
                number: 3,
                title: Some("The Kettle".to_string()),
                summary: Some("<p>Summary</p>".to_string()),
                body_xhtml: "<p class=\"first\">Body</p>".to_string(),
                notes: Some("<p>Before</p>".to_string()),
                end_notes: Some("<p>After</p>".to_string()),
            }
        );
    }

    #[test]
    fn minimal_work_becomes_document() {
        let source = Ao3Html;
        assert!(source.accepts(MINIMAL_WORK.as_bytes()));
        let document = source.to_document(MINIMAL_WORK.as_bytes()).unwrap();
        assert_eq!(document.meta.identifier, "https://archiveofourown.org/works/12345");
        assert_eq!(document.meta.title, "Tea & Biscuits");
        assert_eq!(document.meta.language.as_deref(), Some("en"));
        assert_eq!(document.sections.len(), 1);
        assert_eq!(document.sections[0].number, 1);
        assert!(document.sections[0].body_xhtml.contains("Once upon a time."));
        let Origin::Ao3(version) = &document.origin else {
            panic!("expected AO3 origin");
        };
        assert_eq!(version.metadata.work_id, 12345);
        assert_eq!(version.metadata.rating, Some(Rating::GeneralAudiences));
    }

    #[rstest]
    #[case::empty("")]
    #[case::plain_text("Once upon a time.")]
    #[case::other_html("<html><body><p>Not an AO3 download.</p></body></html>")]
    fn non_ao3_input_is_unsupported(#[case] input: &str) {
        let source = Ao3Html;
        assert!(!source.accepts(input.as_bytes()));
        let error = source.to_document(input.as_bytes()).unwrap_err();
        assert_eq!(*error, ErrorKind::Unsupported);
    }

    #[test]
    fn work_without_chapters_has_no_sections() {
        let input = MINIMAL_WORK
            .replace(r#"<div id="chapters"><div class="userstuff"><p>Once upon a time.</p></div></div>"#, "");
        let error = Ao3Html.to_document(input.as_bytes()).unwrap_err();
        assert_eq!(*error, ErrorKind::NoSections);
    }
}
