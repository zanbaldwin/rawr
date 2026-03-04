//! Configuration error types.
//!
//! Errors are wrapped in [`exn::Exn`] for automatic location tracking and
//! error-tree construction. [`ErrorKind`] variants are designed around caller
//! action rather than internal cause — see each variant's doc for recovery
//! guidance.
//!
// TODO: Definitely going to refactor this later once I've written a few
//       more crates. Designing errors in Rust is **hard** and I don't want
//       to resort to anyhow+thiserror just because I don't want to deal with it.

use derive_more::{Display, Error};
use std::path::PathBuf;

/// A configuration error with automatic location tracking via [`exn::Exn`].
pub type Error = exn::Exn<ErrorKind>;
/// Convenience alias for configuration operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories for configuration loading.
///
/// Variants describe what the caller should *do*, not what went wrong
/// internally. Use pattern matching to decide on recovery strategy, or
/// call [`is_retryable`](ErrorKind::is_retryable) for a quick triage.
#[derive(Debug, Display, Error)]
pub enum ErrorKind {
    /// Config file explicitly specified but not found at the given path.
    #[display("configuration file not found: {}", path.display())]
    NotFound { path: PathBuf },
    /// No config file found in any of the [discovery locations](crate).
    #[display("no configuration file found. Run `rawr init` for setup")]
    NoConfigDiscovered,
    /// Failed to parse or extract configuration from the underlying
    /// [`figment`] provider chain.
    #[display("failed to load configuration: {_0}")]
    Figment(Box<figment::Error>),
    /// One or more [`ConstraintViolation`]s with [`ViolationSeverity::Error`]
    /// severity were produced during post-parse validation.
    #[display("configuration validation failed")]
    Validation { errors: Vec<ConstraintViolation> },
    /// I/O error during file operations.
    #[display("I/O error: {_0}")]
    Io(std::io::Error),
}

impl ErrorKind {
    /// Returns `true` if retrying the operation might succeed (currently
    /// only [`Io`](ErrorKind::Io) errors).
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Io(_))
    }
}

/// A single validation problem found after parsing, tied to a dotted config
/// path (e.g. `"library.targets.import"`).
#[derive(Debug)]
pub struct ConstraintViolation {
    pub severity: ViolationSeverity,
    /// Dotted path into the config tree where the violation was found.
    pub path: String,
    /// Human-readable description of the problem.
    pub message: String,
}
impl ConstraintViolation {
    /// Create an [`Error`](ViolationSeverity::Error)-severity violation.
    pub fn error(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ViolationSeverity::Error,
            path: path.into(),
            message: message.into(),
        }
    }

    /// Create a [`Warning`](ViolationSeverity::Warning)-severity violation.
    pub fn warning(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ViolationSeverity::Warning,
            path: path.into(),
            message: message.into(),
        }
    }

    /// Returns `true` when severity is [`ViolationSeverity::Error`], which
    /// blocks configuration loading.
    pub fn is_fatal(&self) -> bool {
        matches!(self.severity, ViolationSeverity::Error)
    }
}

/// Controls whether a [`ConstraintViolation`] blocks config loading or is
/// surfaced as a non-fatal warning.
#[derive(Debug, PartialEq)]
pub enum ViolationSeverity {
    /// Fatal — prevents the configuration from loading.
    Error,
    /// Non-fatal — returned alongside the loaded [`Config`](crate::models::Config).
    Warning,
}
