use super::{Author, Chapters, Fandom, Language, Rating, SeriesPosition, Tag, Warning};
use time::Date;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// AO3 Work ID (extracted from URL)
    pub work_id: u64,
    /// Work title
    pub title: String,
    /// List of authors (may be empty for anonymous/orphaned works)
    pub authors: Vec<Author>,
    /// List of fandoms the work belongs to
    pub fandoms: Vec<Fandom>,
    /// Series memberships
    pub series: Vec<SeriesPosition>,
    /// Chapter information
    pub chapters: Chapters,
    /// Total word count
    pub words: u64,
    /// Content rating
    pub rating: Option<Rating>,
    /// Archive warnings
    pub warnings: Vec<Warning>,
    /// All tags (relationships, characters, freeform)
    pub tags: Vec<Tag>,
    /// Work summary (converted to Markdown)
    pub summary: Option<String>,
    /// Language of the work
    pub language: Language,
    /// Original publication date
    pub published: Date,
    /// Most recent modification date (update or completion)
    pub last_modified: Date,
}
