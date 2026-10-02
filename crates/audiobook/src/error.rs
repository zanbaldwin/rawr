//! Audiobook Error Types

use derive_more::{Display, Error};

/// An audiobook error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for audiobook operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
#[derive(Debug, Display, Error, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// The section has nothing to speak: no paragraphs, only scene breaks or nothing at all.
    #[display("section {_0} has no spoken text")]
    EmptySection(#[error(not(source))] u32),
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        // Scripts are deterministic: the same section always gives the same result.
        false
    }
}
