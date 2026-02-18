//! Library Error Types
//!
//! This module provides structured errors using `exn` for automatic location
//! tracking and error tree construction. See `ERRORS.md` for design rationale.
//!
//! TODO: Definitely going to refactor this later once I've written a few
//!       more crates. Designing errors in Rust is **hard** and I don't want
//!       to resort to anyhow+thiserror just because I don't want to deal with it.

use derive_more::{Display, Error};

/// A library error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for library operations.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Display, Error)]
pub enum ErrorKind {
    #[display("issue with path generation from template")]
    Template,
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        match self {
            _ => false,
        }
    }
}
