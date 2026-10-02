//! Text-to-speech Error Types

use crate::types::Synthesis;
use derive_more::{Display, Error};

/// A text-to-speech error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for text-to-speech operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
///
/// Providers classify their errors. The caller owns retries and back-off,
/// based on [`is_retryable`](Self::is_retryable).
#[derive(Debug, Display, Error, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// The provider asked us to slow down.
    #[display("rate limited by the provider")]
    RateLimited,
    /// A server error, timeout or connection failure.
    #[display("provider unavailable")]
    Unavailable,
    /// The account or key has no quota left.
    #[display("provider quota exceeded")]
    QuotaExceeded,
    /// The credentials were refused.
    #[display("provider refused the credentials")]
    Unauthorised,
    /// The provider refused the request itself (bad voice, text too long, ...).
    #[display("provider rejected the request: {_0}")]
    Rejected(#[error(not(source))] String),
    /// The response could not be decoded.
    #[display("malformed provider response")]
    MalformedResponse,
    /// The alignment does not match the sent text.
    ///
    /// The audio was still paid for, so the synthesis is kept here for the
    /// caller to store before it fails.
    #[display("alignment does not match the sent text")]
    AlignmentMismatch(#[error(not(source))] Box<Synthesis>),
    /// No voice is configured for this language.
    #[display("no voice configured for language {}", _0.as_deref().unwrap_or("(none)"))]
    NoVoice(#[error(not(source))] Option<String>),
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimited | Self::Unavailable)
    }
}
