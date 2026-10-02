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
