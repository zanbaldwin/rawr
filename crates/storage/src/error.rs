//! Storage Error Types
//!
//! This module provides structured errors using `exn` for automatic location
//! tracking and error tree construction. See `ERRORS.md` for design rationale.
//!
//! TODO: Definitely going to refactor this later once I've written a few
//!       more crates. Designing errors in Rust is **hard** and I don't want
//!       to resort to anyhow+thiserror just because I don't want to deal with it.

use derive_more::{Display, Error};
use rawr_compress::error::{Error as CompressionError, ErrorKind as CompressionErrorKind};
use std::io::Error as IoError;
use std::path::PathBuf;

/// A storage error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for storage operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
///
/// These describe what the caller should *do*, not what went wrong internally.
#[derive(Debug, Display, Error)]
pub enum ErrorKind {
    /// File does not exist
    #[display("file not found: {}", _0.display())]
    NotFound(#[error(not(source))] PathBuf),
    /// Access denied (permissions or credentials)
    #[display("permission denied: {}", _0.display())]
    PermissionDenied(#[error(not(source))] PathBuf),
    /// File already exists (for operations that require new files)
    #[display("file already exists: {}", _0.display())]
    AlreadyExists(#[error(not(source))] PathBuf),
    /// Underlying I/O error
    #[display("I/O error: {_0}")]
    Io(IoError),
    /// Network-related error (S3 connections, etc.)
    #[display("network error: {_0}")]
    Network(#[error(not(source))] String),
    /// Path contains invalid characters or escapes root
    #[display("invalid path: {}", _0.display())]
    InvalidPath(#[error(not(source))] PathBuf),
    /// Backend-specific error
    #[display("backend error: {_0}")]
    BackendError(#[error(not(source))] String),
    /// Compression/decompression error
    #[display("compression error: {_0}")]
    Compression(CompressionErrorKind),
    /// Path rejected by extension filter (e.g. HtmlBackend)
    #[display("filtered path: {}", _0.display())]
    FilteredPath(#[error(not(source))] PathBuf),
}
impl From<IoError> for ErrorKind {
    fn from(err: IoError) -> Self {
        Self::Io(err)
    }
}
impl ErrorKind {
    /// Convert a compression error into a storage error, preserving the
    /// compress crate's `Exn` frame (error tree) as a child in its own
    /// error tree.
    #[track_caller]
    pub fn compression(err: CompressionError) -> Error {
        let inner = (*err).clone();
        err.raise(ErrorKind::Compression(inner))
    }
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Io(_) | Self::Network(_) | Self::BackendError(_))
    }
}
