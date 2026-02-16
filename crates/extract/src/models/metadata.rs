use super::{Author, Chapters, Fandom, Language, Rating, SeriesPosition, Tag, Warning};
use std::collections::HashMap;
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
impl Metadata {
    /// Returns metadata fields as a HashMap for CSS variable injection.
    pub fn to_css_variables(&self) -> HashMap<&'static str, String> {
        /// Format a number with comma separators: 21837 → "21,837"
        fn human_number(n: u64) -> String {
            let s = n.to_string();
            let mut result = String::new();
            for (i, c) in s.chars().rev().enumerate() {
                if i > 0 && i % 3 == 0 {
                    result.insert(0, ',');
                }
                result.insert(0, c);
            }
            result
        }

        let mut map = HashMap::new();
        map.insert("work-id", self.work_id.to_string());
        map.insert("title", self.title.clone());
        map.insert("summary", self.summary.as_ref().map(|s| s.to_string()).unwrap_or_default());
        map.insert("words", human_number(self.words));
        map.insert("chapters-written", self.chapters.written.to_string());
        map.insert("chapters-total", self.chapters.total.map_or("?".into(), |t| t.to_string()));
        map.insert("rating", self.rating.map(|r| r.as_short_str().to_string()).unwrap_or_default());
        map.insert("published", self.published.to_string());
        map.insert("updated", self.last_modified.to_string());
        map
    }
}
