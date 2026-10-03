//! Deserialized configuration model types.
//!
//! [`Config`] is the top-level type produced by [`Config::load`](crate::Validator).
//! It composes [`LibraryConfig`] (import/export behavior), a map of named
//! [`TargetConfig`] storage backends, and optional [`FandomConfig`] for
//! fandom-name normalization.

mod audio;
mod fandom;
mod library;
mod styles;
mod target;

pub use self::audio::{
    AUDIO_PROVIDERS, AudioConfig, AudioProviders, DEFAULT_BITRATE_KBPS, ELEVENLABS_API_KEY_ENV, ElevenLabsConfig,
};
pub use self::fandom::FandomConfig;
pub use self::library::{
    AUDIO_TEMPLATE_SUFFIX, DEFAULT_TEMPLATE_EXPORT, LibraryConfig, LibraryTargets, PathTemplates, audio_template,
};
pub use self::styles::{StyleFormat, Styles};
pub use self::target::TargetConfig;
use serde::Deserialize;
use std::collections::HashMap;

/// Top-level rawr configuration, loaded via [`Config::load`](crate::Loader).
#[derive(Deserialize)]
pub struct Config {
    /// Library behavior: caching, compression, path templates, and which
    /// [`TargetConfig`] to use for import/export/trash.
    pub library: LibraryConfig,
    /// Named storage backends, referenced by [`LibraryTargets`].
    pub targets: HashMap<String, TargetConfig>,
    /// Optional fandom-name normalization and preference rules.
    #[serde(default)]
    pub fandoms: FandomConfig,
    /// Narration and audiobook settings. Configs without `[audio]` load unchanged.
    pub audio: Option<AudioConfig>,
}
