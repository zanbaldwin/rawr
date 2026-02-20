//! Error types for the [`organize`](super) module.
//!
//! Uses [`exn`] for automatic location tracking and error tree construction.
//! See `ERRORS.md` for design rationale.
//!
//! TODO: Definitely going to refactor this later once I've written a few
//!       more crates. Designing errors in Rust is **hard** and I don't want
//!       to resort to anyhow+thiserror just because I don't want to deal with it.

use derive_more::{Display, Error};

/// An organize error with automatic location tracking via [`exn::Exn`].
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for organize operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Classifies the origin of an organize failure.
///
/// Each variant identifies the subsystem that failed, allowing callers to
/// inspect the error tree without matching on opaque strings.
///
/// ### Operational Errors
/// - [`ErrorKind::Template`]
/// - [`ErrorKind::Conflict`]
///
/// ### Dependency Errors
/// - [`ErrorKind::Compression`]
/// - [`ErrorKind::Cache`]
/// - [`ErrorKind::Storage`]
/// - [`ErrorKind::Scan`] - dependency error, but happened during an
///   implicit scan of an unknown file.
#[derive(Debug, Display, Error)]
pub enum ErrorKind {
    /// Decompression or re-compression failed during format conversion.
    Compression,
    /// A cache lookup or update via [`rawr_cache::Repository`] failed.
    Cache,
    /// A storage backend operation (read, write, rename, delete) failed.
    Storage,
    /// The [`PathGenerator`](crate::PathGenerator) could not render a path.
    Template,
    /// A scan was required to resolve a conflict but failed.
    Scan,
    /// Recursive conflict resolution exceeded the depth limit or encountered
    /// an irreconcilable collision.
    Conflict,
    OrganizeFailed,
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        match self {
            _ => false,
        }
    }
}
