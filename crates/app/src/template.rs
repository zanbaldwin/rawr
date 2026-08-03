//! Path-generator and library-context construction.
//!
//! The fandom-selector closure attached here is what makes
//! `{{ fandom }}` in path templates respect the user's configured
//! canonical fandom preferences; before this crate existed the same
//! closure was written out twice in the CLI (import and export).

use crate::backend::{BackendPurpose, backend_by_purpose};
use crate::error::{ErrorKind, Result};
use exn::ResultExt;
use rawr_compress::Compression;
use rawr_config::Config;
use rawr_config::models::FandomConfig;
use rawr_library::{Context as LibraryContext, PathGenerator};

/// Compile a path template and attach the config-driven fandom selector.
pub fn path_generator(template: &str, fandoms: &FandomConfig) -> Result<PathGenerator> {
    let generator = template.parse::<PathGenerator>().or_raise(|| ErrorKind::Template)?;
    let fandoms = fandoms.clone();
    Ok(generator.with_fandom_selector(move |fandom_list| {
        let names: Vec<&str> = fandom_list.iter().map(|f| f.name.as_str()).collect();
        fandoms.preferred_fandom(&names).map(String::from)
    }))
}

/// `config.library.path_templates.import` with the fandom selector.
pub fn import_path_generator(config: &Config) -> Result<PathGenerator> {
    path_generator(&config.library.path_templates.import, &config.fandoms)
}

/// `config.library.path_templates.export` with the fandom selector.
pub fn export_path_generator(config: &Config) -> Result<PathGenerator> {
    path_generator(&config.library.path_templates.export, &config.fandoms)
}

/// Build a [`LibraryContext`]: import path generator, target compression,
/// optional trash backend, and the read-only flag.
pub async fn library_context(
    config: &Config,
    compression: impl Into<Option<Compression>>,
    read_only: bool,
) -> Result<LibraryContext> {
    let generator = import_path_generator(config)?;
    let trash = backend_by_purpose(config, BackendPurpose::Trash, read_only).await?;
    Ok(LibraryContext::new(generator, compression.into(), trash, read_only))
}
