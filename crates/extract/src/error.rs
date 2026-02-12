//! Extraction Error Types
//!
//! This module provides structured errors using `exn` for automatic location
//! tracking and error tree construction. See `ERRORS.md` for design rationale.
//!
//! TODO: Definitely going to refactor this later once I've written a few
//!       more crates. Designing errors in Rust is **hard** and I don't want
//!       to resort to anyhow+thiserror just because I don't want to deal with it.

use derive_more::{Display, Error};

/// An extraction error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for extraction operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
///
/// These describe what the caller should *do*, not what went wrong internally.
#[derive(Debug, Display, Error, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// The HTML structure is too broken to process.
    #[display("malformed HTML")]
    MalformedHtml(#[error(not(source))] String),
    /// The HTML is not a valid AO3 download.
    #[display("invalid AO3 download: missing required structure")]
    InvalidDocument,
    /// A required field could not be found in the document.
    #[display("missing required field: {_0}")]
    MissingField(#[error(not(source))] &'static str),
    /// A field was found but could not be parsed.
    #[display("failed to parse field '{field}', found value: {value}")]
    ParseError {
        /// The field that failed to parse.
        field: &'static str,
        /// Details about the parsing failure.
        value: String,
    },
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        // There are no retryable errors in this crate, the HTML is
        // either valid or its not.
        false
    }
}
