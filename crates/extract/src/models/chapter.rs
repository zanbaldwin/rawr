/// A single chapter's content extracted from an AO3 HTML download.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChapterContent {
    /// Chapter number (1-indexed, in document order).
    pub number: u32,
    /// Chapter title from the heading, if present.
    pub title: Option<String>,
    pub summary: Option<String>,
    /// Raw inner HTML of the chapter body.
    pub body_html: String,
    pub author_notes: Option<String>,
    pub end_notes: Option<String>,
}
