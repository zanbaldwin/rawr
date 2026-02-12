//! Compression Error Types
//!
//! This module provides structured errors using `exn` for automatic location
//! tracking and error tree construction. See `ERRORS.md` for design rationale.
//!
//! TODO: Definitely going to refactor this later once I've written a few
//!       more crates. Designing errors in Rust is **hard** and I don't want
//!       to resort to anyhow+thiserror just because I don't want to deal with it.

use derive_more::{Display, Error};

/// A compression error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for compression operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
///
/// These describe what the caller should *do*, not what went wrong internally.
#[derive(Debug, Display, Error, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// Failed to initialize an encoder/decoder for requested compression format.
    Encoder,
    /// Data is corrupt or malformed. Don't retry with the same input. Used for reading/decoding.
    #[display("invalid or corrupted data")]
    InvalidData,
    /// The requested format is not supported.
    #[display("unsupported format: {_0}")]
    UnsupportedFormat(#[error(not(source))] String),
    /// The requested format is supported but not enabled.
    #[display("disabled format: {_0}")]
    DisabledFormat(#[error(not(source))] String),
    /// An I/O operation failed. Used for writing/encoding.
    #[display("I/O error")]
    Io,
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(self, ErrorKind::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exn::ResultExt;

    #[test]
    fn error_kind_display() {
        assert_eq!(
            ErrorKind::InvalidData.to_string(),
            "invalid or corrupted data"
        );
        assert_eq!(
            ErrorKind::UnsupportedFormat("lz4".to_string()).to_string(),
            "unsupported format: lz4"
        );
        assert_eq!(ErrorKind::Io.to_string(), "I/O error");
    }

    #[test]
    fn error_kind_retryable() {
        assert!(!ErrorKind::InvalidData.is_retryable());
        assert!(!ErrorKind::UnsupportedFormat("zstd".to_string()).is_retryable());
        assert!(ErrorKind::Io.is_retryable());
    }

    #[test]
    fn error_from_result() {
        let result: std::result::Result<(), std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));

        let err: Result<()> = result.or_raise(|| ErrorKind::Io);
        assert!(err.is_err());

        let exn = err.unwrap_err();
        // Exn<E> implements Deref<Target = E>
        assert_eq!(*exn, ErrorKind::Io);
    }
}
