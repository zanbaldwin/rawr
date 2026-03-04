//! Config-file discovery and loading.
//!
//! Implements the full loading pipeline: discover a config file from one of
//! several [sources](ConfigDiscoverySource), parse it with [`figment`],
//! apply [auto-configuration](crate::auto), and run
//! [validation](crate::validation). The public entry point is
//! [`Config::load`].

use crate::APP_NAME;
use crate::auto::Configurator as Autoconfigurator;
use crate::error::{ConstraintViolation, ErrorKind, Result};
use crate::models::Config;
use crate::validation::Validator;
use directories::ProjectDirs;
use figment::Figment;
use figment::providers::{self, Format};
use std::path::{Path, PathBuf};

/// Environment variable checked for an explicit config file path during
/// discovery, before falling back to directory searches.
pub const CONFIG_ENV_VAR: &str = "RAWR_CONFIG";
/// Environment variable prefix for field-level overrides (e.g.
/// `RAWR__LIBRARY__COMPRESSION=zstd`). Double-underscore separates
/// nested keys.
pub const ENV_PREFIX: &str = "RAWR__";
/// File extensions recognized as config files during directory searches.
pub const VALID_EXTENSIONS: [&str; 4] = ["toml", "yaml", "yml", "json"];

/// How a configuration file was discovered.
///
/// Sources are tried in declaration order; the first match wins.
/// [`Explicit`](Self::Explicit) and [`EnvVar`](Self::EnvVar) sources are
/// *required* — if they point to a missing file, loading fails rather than
/// falling through to the next source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigDiscoverySource {
    /// Explicitly specified via `--config` CLI flag.
    Explicit,
    /// From the [`RAWR_CONFIG`](CONFIG_ENV_VAR) environment variable.
    EnvVar,
    /// Found in the current working directory (e.g. `./rawr.yaml`).
    CurrentDir,
    /// Found in the platform-specific config directory
    /// (e.g. `~/.config/rawr/config.yaml` on Linux).
    UserConfig,
}
impl ConfigDiscoverySource {
    /// Whether this source requires the file to exist — `true` for
    /// [`Explicit`](Self::Explicit) and [`EnvVar`](Self::EnvVar).
    pub fn is_required(self) -> bool {
        matches!(self, Self::Explicit | Self::EnvVar)
    }
}

/// Entry point for the configuration loading pipeline.
///
/// Implementors run the full discover → parse → auto-configure → validate
/// sequence described in the [crate-level docs](crate). [`Config`] is the
/// only provided implementation.
pub trait Loader {
    /// Discover, parse, auto-configure, and validate a rawr configuration.
    ///
    /// `explicit` is an optional path from the `--config` CLI flag. When
    /// `None`, discovery proceeds through the remaining
    /// [`ConfigDiscoverySource`] variants in order.
    ///
    /// Returns the validated [`Config`] alongside any non-fatal
    /// [`ConstraintViolation`] warnings. Fatal violations cause an
    /// [`ErrorKind::Validation`] error instead.
    fn load<P: AsRef<Path>>(explicit: impl Into<Option<P>>) -> Result<(Config, Vec<ConstraintViolation>)>;
}
impl Loader for Config {
    fn load<P: AsRef<Path>>(explicit: impl Into<Option<P>>) -> Result<(Config, Vec<ConstraintViolation>)> {
        // Go through discovery sources in order: Explicit, EnvVar, CurrentDir, UserConfig.
        let result = explicit
            .into()
            .map(|p| (p.as_ref().to_path_buf(), ConfigDiscoverySource::Explicit))
            .or_else(|| std::env::var(CONFIG_ENV_VAR).ok().map(|v| (PathBuf::from(v), ConfigDiscoverySource::EnvVar)))
            .or_else(|| search_in_dir(".", APP_NAME).map(|p| (p, ConfigDiscoverySource::CurrentDir)))
            .or_else(|| {
                // Platform-specific:
                // - Linux: `$XDG_CONFIG_HOME/rawr/` or `~/.config/rawr/`
                // - macOS: `~/Library/Application Support/rawr/`
                // - Windows: `C:\Users\<User>\AppData\Roaming\rawr\`
                ProjectDirs::from("", "", APP_NAME)
                    .and_then(|d| search_in_dir(d.config_dir(), "config"))
                    .map(|p| (p, ConfigDiscoverySource::UserConfig))
            });
        let path = match result {
            Some((path, _)) if !path.extension().is_some_and(|e| VALID_EXTENSIONS.iter().any(|&ext| e == ext)) => {
                exn::bail!(ErrorKind::NotFound { path })
            },
            Some((path, _)) if path.exists() => path,
            Some((path, source)) if source.is_required() => exn::bail!(ErrorKind::NotFound { path }),
            _ => exn::bail!(ErrorKind::NoConfigDiscovered),
        };

        // Load config from file, and merge overrides from environment variables.
        let figment = Figment::new();
        // Safety: the extension will be a valid str because we checked above.
        let figment = match path.extension().unwrap().to_str().unwrap() {
            "toml" => figment.merge(providers::Toml::file(path)),
            "yaml" | "yml" => figment.merge(providers::Yaml::file(path)),
            "json" => figment.merge(providers::Json::file(path)),
            _ => figment,
        }
        .merge(providers::Env::prefixed(ENV_PREFIX).split("__"));

        // Autoconfigure values that are required for hydration, but can be inferred from other values.
        let autoconfigured = Figment::from(Autoconfigurator::from(figment));
        let hydrated: Config = autoconfigured.extract().map_err(|e| exn::Exn::from(ErrorKind::Figment(Box::new(e))))?;

        let (errors, warnings) = hydrated.validate().into_iter().partition::<Vec<_>, _>(|v| v.is_fatal());
        if !errors.is_empty() {
            exn::bail!(ErrorKind::Validation { errors });
        }
        Ok((hydrated, warnings))
    }
}

/// Search `directory` for a file named `{name}.{ext}` with any of the
/// [`VALID_EXTENSIONS`], returning the first match.
fn search_in_dir(directory: impl AsRef<Path>, name: impl AsRef<str>) -> Option<PathBuf> {
    let directory = directory.as_ref();
    for ext in VALID_EXTENSIONS {
        let file = directory.join(format!("{}.{ext}", name.as_ref()));
        if file.exists() {
            return Some(file);
        }
    }
    None
}
