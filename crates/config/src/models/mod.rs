//! Deserialized configuration model types.
//!
//! [`Config`] is the top-level type produced by [`Config::load`](crate::Validator).
//! It composes [`LibraryConfig`] (import/export behavior), a map of named
//! [`TargetConfig`] storage backends, and optional [`FandomConfig`] for
//! fandom-name normalization.

mod fandom;
mod library;
mod target;

pub use self::fandom::FandomConfig;
pub use self::library::{LibraryConfig, LibraryTargets, PathTemplates};
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
}
