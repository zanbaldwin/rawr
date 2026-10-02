use crate::document::{Document, DocumentMeta, Origin, Rating, Section};
use crate::error::{ErrorKind, Result};
use crate::source::Source;
use exn::ResultExt;
use rawr_extract::Extractor;
use rawr_extract::models::{ChapterContent, Metadata, Rating as Ao3Rating};

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
        let sections: Vec<Section> =
            Extractor::from_html(input).chapters_xhtml().into_iter().map(Section::from_ao3_chapter).collect();
        if sections.is_empty() {
            exn::bail!(ErrorKind::NoSections);
        }
        Ok(Document {
            meta: DocumentMeta::from_ao3_version(&version.metadata),
            sections,
            origin: Origin::Ao3(Box::new(version)),
        })
    }
}

impl DocumentMeta {
    fn from_ao3_version(metadata: &Metadata) -> Self {
        Self {
            identifier: format!("{WORK_URL_PREFIX}{}", metadata.work_id),
            title: metadata.title.clone(),
            authors: metadata.authors.iter().map(ToString::to_string).collect(),
            language: metadata.language.iso_code.map(String::from),
            summary: metadata.summary.clone(),
            rating: match metadata.rating {
                Some(Ao3Rating::GeneralAudiences) => Rating::General,
                Some(Ao3Rating::TeenAndUp) => Rating::Teen,
                Some(Ao3Rating::Mature) => Rating::Mature,
                Some(Ao3Rating::Explicit) => Rating::Explicit,
                Some(Ao3Rating::NotRated) | None => Rating::Unrated,
            },
        }
    }
}
impl Section {
    fn from_ao3_chapter(chapter: ChapterContent) -> Self {
        Self {
            number: chapter.number,
            title: chapter.title,
            summary: chapter.summary,
            body_xhtml: chapter.body_html,
            notes: chapter.author_notes,
            end_notes: chapter.end_notes,
        }
    }
}
