//! Source Error Types

use derive_more::{Display, Error};

/// A source error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for source operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
#[derive(Debug, Display, Error, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// The source does not understand this input; try another source.
    #[display("input not supported by this source")]
    Unsupported,
    /// The input looked supported, but extraction failed.
    #[display("failed to extract document from input")]
    Extract,
    /// The document has no sections to narrate.
    #[display("document has no sections")]
    NoSections,
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        // Parsing is deterministic: the same bytes always give the same result.
        false
    }
}
