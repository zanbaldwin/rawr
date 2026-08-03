//! Web Server Error Types
//!
//! This module provides structured errors using `exn` for automatic location
//! tracking and error tree construction. See `ERRORS.md` for design rationale.
//!
//! TODO: Definitely going to refactor this later once I've written a few
//!       more crates. Designing errors in Rust is **hard** and I don't want
//!       to resort to anyhow+thiserror just because I don't want to deal with it.

use derive_more::{Display, Error};

/// A web server error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for web server operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
///
/// These describe what the caller should *do*, not what went wrong internally.
/// Each maps to an HTTP status; the mapping lives with the response layer.
#[derive(Debug, Display, Error)]
pub enum ErrorKind {
    /// The requested work/version is not in the library.
    #[display("that work is not in your library")]
    NotFound,
    /// The request could not be understood.
    #[display("the request could not be understood: {_0}")]
    BadRequest(#[error(not(source))] &'static str),
    /// The request body is not a type this endpoint accepts.
    #[display("this endpoint does not accept that content type")]
    Unsupported,
    /// The request body exceeds the configured limit.
    #[display("the upload is too large")]
    TooLarge,
    /// An uploaded file could not be read.
    #[display("the upload could not be read: {_0}")]
    Upload(#[error(not(source))] String),
    /// The library index could not be built.
    #[display("the library index could not be built")]
    Index,
    /// The library cache could not be queried.
    #[display("the library cache could not be queried")]
    Cache,
    /// The work's file could not be read from storage.
    #[display("the work's file could not be read from storage")]
    Storage,
    /// The work could not be converted for download.
    #[display("the work could not be converted for download")]
    Render,
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Cache | Self::Storage)
    }
}
