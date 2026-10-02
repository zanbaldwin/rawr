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
    /// The text-to-speech provider failed; the provider's error is the source.
    #[display("text-to-speech provider failed")]
    Provider,
    /// Audio in a form the codec does not handle (sample rate, channels, length).
    #[display("unsupported audio: {_0}")]
    UnsupportedAudio(#[error(not(source))] String),
    /// Audio that cannot be read.
    #[display("malformed audio")]
    MalformedAudio,
    /// The audio encoder or decoder failed.
    #[display("audio codec failed")]
    Codec,
    /// A batch has no usable recording (missing, or with an alignment mismatch).
    #[display("no usable recording for batch {_0}")]
    MissingRecording(#[error(not(source))] String),
    /// A recording has no alignment, so its sentences cannot be timed.
    #[display("recording {_0} has no alignment")]
    MissingAlignment(#[error(not(source))] String),
    /// The recordings storage failed.
    #[display("recordings storage failed")]
    Storage,
    /// A recording's metadata cannot be read.
    #[display("recording {_0} is corrupt")]
    CorruptRecording(#[error(not(source))] String),
    /// A ledger cannot be read.
    #[display("ledger {_0} is corrupt")]
    CorruptLedger(#[error(not(source))] String),
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        // Scripts are deterministic: the same section always gives the same result.
        false
    }
}
