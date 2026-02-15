//! Cache Error Types
//!
//! This module provides structured errors using `exn` for automatic location
//! tracking and error tree construction. See `ERRORS.md` for design rationale.
//!
//! TODO: Definitely going to refactor this later once I've written a few
//!       more crates. Designing errors in Rust is **hard** and I don't want
//!       to resort to anyhow+thiserror just because I don't want to deal with it.

use derive_more::{Display, Error};
use std::path::PathBuf;

/// A cache error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for cache operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
///
/// These describe what the caller should *do*, not what went wrong internally.
#[derive(Debug, Display, Error)]
pub enum ErrorKind {
    #[display("database error")]
    Database,
    #[display("database migration error")]
    Migration,
    #[display("file not found: ({_0}, {})", _1.display())]
    FileNotFound(#[error(not(source))] String, PathBuf),
    #[display("version not found: ({_0})")]
    VersionNotFound(#[error(not(source))] String),
    /// Serialization/deserialization error.
    #[display("invalid cache data")]
    InvalidData,
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        false
    }
}
