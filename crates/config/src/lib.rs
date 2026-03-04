//! Configuration loading and validation for **rawr**.
//!
//! This crate discovers, parses, and validates a rawr configuration file from
//! one of several sources (CLI flag, environment variable, working directory, or
//! platform config directory). It supports YAML, TOML, and JSON formats and
//! merges environment-variable overrides via [`figment`].
//!
//! The loading pipeline is:
//!
//! 1. **Discovery** — locate a config file through a priority-ordered search.
//! 2. **Parsing** — deserialize with figment, merging `RAWR__`-prefixed env vars.
//! 3. **Auto-configuration** — fill in values that can be inferred from context
//!    (e.g. defaulting the export target to the import target).
//! 4. **Validation** — check cross-field constraints and emit errors or warnings.
//!
//! # Usage
//!
//! ```no_run
//! use rawr_config::{Loader, Config};
//!
//! let (config, warnings) = Config::load::<std::path::PathBuf>(None)?;
//! for w in &warnings {
//!     eprintln!("warning: {}", w.message);
//! }
//! # Ok::<(), rawr_config::error::Error>(())
//! ```

pub(crate) mod auto;
pub mod error;
mod loader;
mod maybe;
pub mod models;
mod validation;

pub use crate::loader::Loader;
pub use crate::models::Config;
pub use crate::validation::Validator;

/// Application name used for config-file discovery and platform directory resolution.
pub const APP_NAME: &str = "rawr";
