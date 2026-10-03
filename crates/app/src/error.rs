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
    /// The cache database could not be opened or queried.
    #[display("the cache database could not be used")]
    Cache,
    /// No version of the work is in the library.
    #[display("work {_0} not found in library")]
    WorkNotFound(#[error(not(source))] u64),
    /// The work has no version with this CRC32.
    #[display("work {work} has no version with CRC32 {crc32:08x}")]
    VersionNotFound { work: u64, crc32: u32 },
    /// No file is recorded at this path in the target.
    #[display("\"{path}\" not found in target \"{target}\"")]
    FileNotFound { target: String, path: String },
    /// A library file exists but could not be read or decompressed.
    #[display("library file `{_0}` could not be read")]
    Read(#[error(not(source))] String),
    /// The command needs an `[audio]` table with a provider.
    #[cfg(feature = "audiobook")]
    #[display("narration needs an `[audio]` table with a provider in your config")]
    AudioNotConfigured,
    /// `audio.provider` names a provider this build does not have.
    #[cfg(feature = "audiobook")]
    #[display("unknown audio provider `{_0}`")]
    UnknownProvider(#[error(not(source))] String),
    /// Narration needs the provider's API key.
    #[cfg(feature = "audiobook")]
    #[display("no API key for `{_0}`: set `audio.providers.{_0}.api_key` in your config, or ELEVENLABS_API_KEY")]
    MissingApiKey(#[error(not(source))] &'static str),
    /// The text-to-speech provider could not be set up.
    #[cfg(feature = "audiobook")]
    #[display("the text-to-speech provider could not be set up")]
    Synthesiser,
    /// The recordings directory could not be opened.
    #[cfg(feature = "audiobook")]
    #[display("the recordings directory `{_0}` could not be opened")]
    Recordings(#[error(not(source))] String),
    /// The work's HTML could not be turned into a document.
    #[cfg(feature = "audiobook")]
    #[display("the work could not be read for narration")]
    Document,
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
