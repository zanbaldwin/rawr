//! Read Log Error Types
//!
//! This module provides structured errors using `exn` for automatic location
//! tracking and error tree construction. See `ERRORS.md` for design rationale.

use derive_more::{Display, Error};

/// A read log error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for read log operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
///
/// These describe what the caller should *do*, not what went wrong internally.
#[derive(Debug, Display, Error)]
pub enum ErrorKind {
    /// The read log could not be read from or written to storage.
    #[display("read log `{_0}` could not be accessed in storage")]
    Storage(#[error(not(source))] String),
    /// A row in the read log is malformed; fix the file by hand.
    #[display("read log line {line}: {reason}")]
    Parse {
        line: u64,
        #[error(not(source))]
        reason: String,
    },
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Storage(_))
    }
}
