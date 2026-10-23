//! Read log: a record of which works have been read, and when.
//!
//! The log is a CSV file with the columns `date,work_id,chapter_update`,
//! stored in the `library.targets.reads` target. Unlike the cache database,
//! it is user data and cannot be rebuilt by scanning the library.
//!
//! - `date` is an ISO `YYYY-MM-DD` date.
//! - `work_id` is a bare AO3 work ID.
//! - `chapter_update` is empty for a full read. Any other value (rawr writes
//!   `Y`) marks a read of only a new chapter.
//!
//! Every row is one read event, so re-reads and several chapter updates on
//! the same day are separate rows.

pub mod error;

use crate::error::{ErrorKind, Result};
use exn::ResultExt;
use rawr_storage::{BackendHandle, ValidPath};
use std::collections::HashMap;
use std::fmt::Write;
use time::Date;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

const HEADER: &str = "date,work_id,chapter_update";
const DATE_FORMAT: &[BorrowedFormatItem<'static>] = format_description!("[year]-[month]-[day]");

/// One read event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadEntry {
    pub date: Date,
    pub work_id: u64,
    /// Only a new chapter was read, not the whole work.
    pub chapter_update: bool,
}

/// Aggregated read events for a single work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadSummary {
    /// Number of full reads.
    pub full: u32,
    /// Number of chapter-update reads.
    pub updates: u32,
    /// Most recent read of either kind.
    pub last: Date,
}

/// The read log, in file order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReadLog {
    entries: Vec<ReadEntry>,
}

impl ReadLog {
    /// Load the read log from storage. A missing file is an empty log.
    pub async fn load(backend: &BackendHandle, path: &ValidPath) -> Result<Self> {
        let storage = || ErrorKind::Storage(path.to_string());
        if !backend.exists(path).await.or_raise(storage)? {
            return Ok(Self::default());
        }
        Self::parse(&backend.read(path).await.or_raise(storage)?)
    }

    /// Write the read log to storage, through a temporary file so that a
    /// failed write does not truncate the existing log.
    pub async fn save(&self, backend: &BackendHandle, path: &ValidPath) -> Result<()> {
        let storage = || ErrorKind::Storage(path.to_string());
        let temp = ValidPath::new(format!("{path}.tmp")).or_raise(storage)?;
        backend.write(&temp, &self.to_csv()).await.or_raise(storage)?;
        backend.rename(&temp, path).await.or_raise(storage)
    }

    /// Parse CSV data. The header line is optional, and a missing
    /// `chapter_update` column means a full read.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut reader =
            csv::ReaderBuilder::new().has_headers(false).flexible(true).trim(csv::Trim::All).from_reader(data);
        let mut entries = Vec::new();
        for (index, record) in reader.records().enumerate() {
            let record = record.map_err(|e| {
                let line = e.position().map_or(0, |p| p.line());
                ErrorKind::Parse { line, reason: e.to_string() }
            })?;
            if index == 0 && record.get(0).is_some_and(|field| field.eq_ignore_ascii_case("date")) {
                continue;
            }
            entries.push(parse_record(&record)?);
        }
        Ok(Self { entries })
    }

    /// Serialise to CSV, with a header line and `Y` for chapter updates.
    pub fn to_csv(&self) -> Vec<u8> {
        let mut out = String::with_capacity((self.entries.len() + 1) * 24);
        out.push_str(HEADER);
        out.push('\n');
        for entry in &self.entries {
            let flag = if entry.chapter_update { "Y" } else { "" };
            // Writing to a String is infallible.
            let _ = writeln!(out, "{},{},{}", entry.date, entry.work_id, flag);
        }
        out.into_bytes()
    }

    pub fn entries(&self) -> &[ReadEntry] {
        &self.entries
    }

    /// Append a read event.
    pub fn push(&mut self, entry: ReadEntry) {
        self.entries.push(entry);
    }

    /// Remove the last event equal to `entry`. Returns `false` if there is
    /// no such event.
    pub fn remove_last(&mut self, entry: &ReadEntry) -> bool {
        match self.entries.iter().rposition(|e| e == entry) {
            Some(index) => {
                self.entries.remove(index);
                true
            },
            None => false,
        }
    }

    /// Read counts and the most recent read date, per work.
    pub fn summary(&self) -> HashMap<u64, ReadSummary> {
        let mut summary: HashMap<u64, ReadSummary> = HashMap::new();
        for entry in &self.entries {
            let work = summary.entry(entry.work_id).or_insert(ReadSummary { full: 0, updates: 0, last: entry.date });
            match entry.chapter_update {
                true => work.updates += 1,
                false => work.full += 1,
            }
            work.last = work.last.max(entry.date);
        }
        summary
    }
}

fn parse_record(record: &csv::StringRecord) -> Result<ReadEntry> {
    let line = record.position().map_or(0, |p| p.line());
    let field = |index: usize| record.get(index).unwrap_or_default();
    let Ok(date) = Date::parse(field(0), DATE_FORMAT) else {
        exn::bail!(ErrorKind::Parse {
            line,
            reason: format!("`{}` is not a YYYY-MM-DD date", field(0)),
        });
    };
    let Ok(work_id) = field(1).parse::<u64>() else {
        exn::bail!(ErrorKind::Parse {
            line,
            reason: format!("`{}` is not a work ID", field(1)),
        });
    };
    Ok(ReadEntry {
        date,
        work_id,
        chapter_update: !field(2).is_empty(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawr_storage::backend::MockBackend;
    use std::sync::Arc;
    use time::macros::date;

    fn entry(date: Date, work_id: u64, chapter_update: bool) -> ReadEntry {
        ReadEntry { date, work_id, chapter_update }
    }

    fn parse_error_line(data: &str) -> u64 {
        let err = ReadLog::parse(data.as_bytes()).unwrap_err();
        match &*err {
            ErrorKind::Parse { line, .. } => *line,
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn round_trip() {
        let data = "date,work_id,chapter_update\n2022-01-01,36269506,\n2023-09-04,43544760,Y\n";
        let log = ReadLog::parse(data.as_bytes()).unwrap();
        assert_eq!(
            log.entries(),
            [
                entry(date!(2022 - 01 - 01), 36269506, false),
                entry(date!(2023 - 09 - 04), 43544760, true)
            ]
        );
        assert_eq!(String::from_utf8(log.to_csv()).unwrap(), data);
    }

    #[test]
    fn header_and_third_column_are_optional() {
        let log = ReadLog::parse(b"2022-01-01,1\n\n2022-01-02,2,\n").unwrap();
        assert_eq!(
            log.entries(),
            [
                entry(date!(2022 - 01 - 01), 1, false),
                entry(date!(2022 - 01 - 02), 2, false)
            ]
        );
    }

    #[test]
    fn any_non_empty_flag_is_a_chapter_update() {
        let log = ReadLog::parse(b"2022-01-01,1,\n2022-01-01,2,Y\n2022-01-01,3,Yes\n2022-01-01,4,  \n").unwrap();
        let flags: Vec<bool> = log.entries().iter().map(|e| e.chapter_update).collect();
        assert_eq!(flags, [false, true, true, false]);
    }

    #[test]
    fn quoted_fields() {
        let log = ReadLog::parse(b"\"2022-01-01\",\"1\",\"Y\"\n").unwrap();
        assert_eq!(log.entries(), [entry(date!(2022 - 01 - 01), 1, true)]);
    }

    #[test]
    fn bad_date_names_the_line() {
        assert_eq!(parse_error_line("date,work_id,chapter_update\n2022-01-01,1,\n44562,2,\n"), 3);
        assert_eq!(parse_error_line("2022-01-01,1,\n2022-13-01,2,\n"), 2);
    }

    #[test]
    fn bad_work_id_names_the_line() {
        assert_eq!(parse_error_line("2022-01-01,https://archiveofourown.org/works/1,\n"), 1);
    }

    #[test]
    fn repeated_rows_are_kept_and_summarised() {
        let log = ReadLog::parse(b"2023-09-04,7,Y\n2023-09-04,7,Y\n2023-01-01,7,\n2023-02-01,8,\n").unwrap();
        assert_eq!(log.entries().len(), 4);
        let summary = log.summary();
        assert_eq!(
            summary[&7],
            ReadSummary {
                full: 1,
                updates: 2,
                last: date!(2023 - 09 - 04)
            }
        );
        assert_eq!(
            summary[&8],
            ReadSummary {
                full: 1,
                updates: 0,
                last: date!(2023 - 02 - 01)
            }
        );
    }

    #[test]
    fn remove_last_removes_only_the_last_match() {
        let mut log = ReadLog::default();
        log.push(entry(date!(2024 - 01 - 01), 1, false));
        log.push(entry(date!(2024 - 01 - 01), 2, false));
        log.push(entry(date!(2024 - 01 - 01), 1, false));
        assert!(log.remove_last(&entry(date!(2024 - 01 - 01), 1, false)));
        assert_eq!(
            log.entries(),
            [
                entry(date!(2024 - 01 - 01), 1, false),
                entry(date!(2024 - 01 - 01), 2, false)
            ]
        );
        assert!(!log.remove_last(&entry(date!(2024 - 01 - 01), 1, true)));
    }

    #[tokio::test]
    async fn load_missing_file_is_empty_and_save_round_trips() {
        let backend: BackendHandle = Arc::new(MockBackend::with_data(Vec::<(&str, Vec<u8>)>::new()).await);
        let path = ValidPath::new("reads.csv").unwrap();
        let mut log = ReadLog::load(&backend, &path).await.unwrap();
        assert_eq!(log, ReadLog::default());

        log.push(entry(date!(2025 - 07 - 10), 66996232, true));
        log.save(&backend, &path).await.unwrap();
        assert_eq!(ReadLog::load(&backend, &path).await.unwrap(), log);
        assert!(!backend.exists(&ValidPath::new("reads.csv.tmp").unwrap()).await.unwrap());
    }
}
