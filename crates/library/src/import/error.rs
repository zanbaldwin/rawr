//! Error types for the [`import`](super) module.
//!
//! Uses [`exn`] for automatic location tracking and error tree construction.
//! See `ERRORS.md` for design rationale.
//!
//! TODO: Definitely going to refactor this later once I've written a few
//!       more crates. Designing errors in Rust is **hard** and I don't want
//!       to resort to anyhow+thiserror just because I don't want to deal with it.

use derive_more::{Display, Error};

/// An import error with automatic location tracking via [`exn::Exn`].
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for import operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Classifies the origin of an import failure.
#[derive(Debug, Display, Error)]
pub enum ErrorKind {
    /// Decompression or re-compression failed during format conversion.
    Compression,
    /// A cache lookup or update via the
    /// [cache repository](rawr_cache::Repository) failed.
    Cache,
    /// A storage backend operation (read, write, rename, delete) failed.
    Storage,
    /// The [`PathGenerator`](crate::PathGenerator) could not render a path.
    Template,
    /// Importing the file required organizing others out of the way.
    Organize,
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        match self {
            _ => false,
        }
    }
}
