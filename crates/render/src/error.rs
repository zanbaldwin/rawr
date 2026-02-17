//! Render Error Types
//!
//! This module provides structured errors using `exn` for automatic location
//! tracking and error tree construction. See `ERRORS.md` for design rationale.
//!
//! TODO: Definitely going to refactor this later once I've written a few
//!       more crates. Designing errors in Rust is **hard** and I don't want
//!       to resort to anyhow+thiserror just because I don't want to deal with it.

use derive_more::{Display, Error};

/// A render error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for render operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
///
/// These describe what the caller should *do*, not what went wrong internally.
#[derive(Debug, Display, Error)]
pub enum ErrorKind {
    #[display("chrome/chromium not detected on your system")]
    ChromeNotFound,
    ChromeTimeout,
    /// Chrome exited with a non-zero exit code.
    /// If the exit code is zero, then Chrome either timed-out, killed by signal, or crashed.
    #[display("Chrome exited with code: {_0}")]
    ChromeFailed(#[error(not(source))] i32),
    /// Asset was not loadable (either file or builtin).
    AssetNotFound(#[error(not(source))] String),
    Io,
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        false
    }
}
