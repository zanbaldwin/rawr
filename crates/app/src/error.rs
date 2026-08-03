//! Application Wiring Error Types
//!
//! This module provides structured errors using `exn` for automatic location
//! tracking and error tree construction. See `ERRORS.md` for design rationale.
//!
//! TODO: Definitely going to refactor this later once I've written a few
//!       more crates. Designing errors in Rust is **hard** and I don't want
//!       to resort to anyhow+thiserror just because I don't want to deal with it.

use derive_more::{Display, Error};

/// A wiring error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for wiring operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
///
/// These describe what the caller should *do*, not what went wrong internally.
#[derive(Debug, Display, Error)]
pub enum ErrorKind {
    /// The named target has no entry in the config's `targets` map.
    #[display("storage target `{_0}` is not defined under `targets` in your config")]
    UnknownTarget(#[error(not(source))] String),
    /// The target is configured, but this binary was compiled without the
    /// feature needed to talk to it.
    #[display("target `{target}` needs the `{feature}` feature; recompile with `--features {feature}`")]
    UnavailableBackend { target: String, feature: &'static str },
    /// The backend exists in config but could not be opened.
    #[display("storage backend `{_0}` could not be opened")]
    Backend(#[error(not(source))] String),
    /// A `library.path_templates` entry failed to compile.
    #[display("`library.path_templates` could not be compiled")]
    Template,
    /// A `library.styles` entry failed to load.
    #[cfg(feature = "render")]
    #[display("a stylesheet in `library.styles` could not be loaded")]
    Style,
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        false
    }
}
