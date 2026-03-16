use std::fmt::{Display, Formatter, Result as FmtResult};

/// Chapter count information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChapterCount {
    /// Number of chapters currently posted
    pub written: u32,
    /// Expected total chapters (None if unknown/`?`)
    pub total: Option<u32>,
}
impl ChapterCount {
    pub fn new(written: u32, total: Option<u32>) -> Self {
        Self { written, total }
    }
    /// Returns true if the work is complete (planned chapters have been written).
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.total.is_some_and(|t| self.written >= t)
    }
}
impl From<(u32, u32)> for ChapterCount {
    fn from((written, total): (u32, u32)) -> Self {
        ChapterCount::new(written, Some(total))
    }
}
impl From<(u32, Option<u32>)> for ChapterCount {
    fn from((written, total): (u32, Option<u32>)) -> Self {
        ChapterCount::new(written, total)
    }
}
impl Display for ChapterCount {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self.total {
            Some(total) => write!(f, "{}/{total}", self.written),
            None => write!(f, "{}/?", self.written),
        }
    }
}

/// A single chapter's content extracted from an AO3 HTML download.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg(feature = "chapters")]
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
