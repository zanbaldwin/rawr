use crate::consts;
use crate::error::{ErrorKind, Result};
use crate::models::Chapters;
use exn::{OptionExt, ResultExt};
use time::{Date, Month};
use tracing::instrument;

#[derive(Debug)]
pub struct Stats {
    text: String,
}
impl Stats {
    pub(crate) fn new(text: String) -> Self {
        Self { text }
    }

    /// Extracts chapter information from stats.
    #[instrument(level = "trace")]
    pub fn chapters(&self) -> Result<Chapters> {
        let captures =
            consts::CHAPTERS_REGEX.captures(&self.text).ok_or_raise(|| ErrorKind::MissingField("chapters"))?;
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
    pub fn words(&self) -> Result<u64> {
        let captures =
            consts::WORDS_REGEX.captures(&self.text).ok_or_raise(|| ErrorKind::MissingField("word_count"))?;
        let word_str = captures.get(1).unwrap().as_str().replace(',', "");
        word_str.parse::<u64>().or_raise(|| ErrorKind::ParseError {
            field: "word_count",
            value: "invalid word count".to_string(),
        })
    }

    /// Extracts dates (published, last_modified) from stats.
    #[instrument(level = "trace")]
    pub fn dates(&self) -> Result<(Date, Date)> {
        let mut published: Option<Date> = None;
        let mut last_modified: Option<Date> = None;
        for captures in consts::DATE_REGEX.captures_iter(&self.text) {
            let year: i32 = captures.get(2).unwrap().as_str().parse::<i32>().or_raise(|| ErrorKind::ParseError {
                field: "date-year",
                value: "invalid year number".to_string(),
            })?;
            let month: u8 = captures.get(3).unwrap().as_str().parse::<u8>().or_raise(|| ErrorKind::ParseError {
                field: "date-month",
                value: "invalid month number".to_string(),
            })?;
            let day: u8 = captures.get(4).unwrap().as_str().parse::<u8>().or_raise(|| ErrorKind::ParseError {
                field: "date-day",
                value: "invalid date number".to_string(),
            })?;
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
}
impl From<String> for Stats {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
