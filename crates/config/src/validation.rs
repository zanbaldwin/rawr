//! Post-parse configuration validation.
//!
//! Runs cross-field checks that can't be expressed through serde alone:
//! target references resolve to defined targets, local directories are
//! accessible, S3 credentials are non-empty, fandom rename maps don't
//! contain duplicates, etc.
//!
//! Validation produces [`ConstraintViolation`]s with either
//! [`Error`](crate::error::ViolationSeverity::Error) (fatal) or
//! [`Warning`](crate::error::ViolationSeverity::Warning) severity.
//! The [loader](crate::loader) partitions these: errors abort loading,
//! warnings are returned alongside the valid [`Config`].

use crate::error::ConstraintViolation;
use crate::models::{Config, TargetConfig};
use figment::value::magic::RelativePathBuf;

/// Validates a deserialized configuration, producing any
/// [`ConstraintViolation`]s found.
pub trait Validator {
    /// Check this configuration value for constraint violations.
    ///
    /// Returns all violations (both fatal errors and warnings) in a
    /// single pass. The caller is responsible for partitioning by
    /// severity.
    fn validate(&self) -> Vec<ConstraintViolation>;
}

impl Validator for Config {
    fn validate(&self) -> Vec<ConstraintViolation> {
        let mut errors = Vec::new();
        validate_library_target(self, &mut errors);
        validate_targets(self, &mut errors);
        validate_database(self, &mut errors);
        check_duplicate_fandom_renames(self, &mut errors);
        check_prefs_not_in_renames(self, &mut errors);
        errors
    }
}

fn validate_library_target(config: &Config, errors: &mut Vec<ConstraintViolation>) {
    if !config.targets.contains_key(&config.library.targets.import) {
        errors.push(ConstraintViolation::error(
            "library.targets.import",
            format!(
                "references undefined target '{}'. Available targets: {}",
                config.library.targets.import,
                config.targets.keys().cloned().collect::<Vec<_>>().join(", ")
            ),
        ));
    }
}

fn validate_targets(config: &Config, errors: &mut Vec<ConstraintViolation>) {
    for (name, target) in &config.targets {
        validate_target(name, target, errors);
    }
}

/// Validate a single storage target configuration.
fn validate_target(name: &str, target: &TargetConfig, errors: &mut Vec<ConstraintViolation>) {
    match target {
        TargetConfig::Local { directory, auto_create } => validate_local_target(name, directory, *auto_create, errors),
        TargetConfig::S3 { bucket, region, key_id, key_secret, .. } => {
            validate_s3_target(name, bucket, region, key_id, key_secret, errors)
        },
    }
}

fn validate_local_target(
    name: &str,
    directory: &RelativePathBuf,
    auto_create: bool,
    errors: &mut Vec<ConstraintViolation>,
) {
    let path = directory.relative();
    let path = match path.is_relative() {
        true => std::env::current_dir().map(|cwd| cwd.join(&path)).unwrap_or(path),
        false => path,
    };
    if path.exists() && !path.is_dir() {
        errors.push(ConstraintViolation::error(
            format!("targets.{name}.directory"),
            format!("path exists but is not a directory: {}", path.display()),
        ));
    } else if !path.exists() && !auto_create {
        errors.push(ConstraintViolation::warning(
            format!("targets.{name}.directory"),
            format!("directory does not exist, but auto_create is disabled: {}", path.display()),
        ));
    }
}

fn validate_s3_target(
    name: &str,
    bucket: &str,
    region: &str,
    key_id: &str,
    key_secret: &str,
    errors: &mut Vec<ConstraintViolation>,
) {
    // S3 targets must have required fields (non-empty)
    if bucket.is_empty() {
        errors.push(ConstraintViolation::error(format!("targets.{name}.bucket"), "cannot be empty"));
    }
    if region.is_empty() {
        errors.push(ConstraintViolation::error(format!("targets.{name}.region"), "cannot be empty"));
    }
    if key_id.is_empty() {
        errors.push(ConstraintViolation::error(format!("targets.{name}.key_id"), "cannot be empty"));
    }
    if key_secret.is_empty() {
        errors.push(ConstraintViolation::error(format!("targets.{name}.key_secret"), "cannot be empty"));
    }
}

fn validate_database(config: &Config, errors: &mut Vec<ConstraintViolation>) {
    let path = config.library.cache.relative();

    if path.exists() && !path.is_file() {
        errors.push(ConstraintViolation::error("library.cache", "cache database points to a non-file"));
        return;
    }
    let Some(parent) = path.parent() else {
        errors.push(ConstraintViolation::error("library.cache", "cache database is located in an unknown directory"));
        return;
    };
    if !parent.exists() || !parent.is_dir() {
        errors.push(ConstraintViolation::error(
            "library.cache",
            format!("parent directory doesn't exist; create directory '{}' and the cache database will be created automatically", parent.to_string_lossy()),
        ));
    }
}

fn check_duplicate_fandom_renames(config: &Config, warnings: &mut Vec<ConstraintViolation>) {
    let mut seen_aliases: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for (display_name, aliases) in &config.fandoms.renames {
        for alias in aliases {
            if let Some(other_display) = seen_aliases.get(alias.as_str()) {
                warnings.push(ConstraintViolation::warning(
                    "fandoms.renames",
                    format!("fandom '{}' appears in both '{}' and '{}'", alias, other_display, display_name),
                ));
            } else {
                seen_aliases.insert(alias, display_name);
            }
        }
    }
}

fn check_prefs_not_in_renames(config: &Config, warnings: &mut Vec<ConstraintViolation>) {
    for pref in &config.fandoms.preferences {
        if !config.fandoms.renames.contains_key(pref) {
            warnings.push(ConstraintViolation::warning(
                "fandoms.preferences",
                format!("preference '{}' does not match any rename group", pref),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ViolationSeverity;
    use crate::maybe::MaybeFile;
    use crate::models::{FandomConfig, LibraryConfig, LibraryTargets, PathTemplates};
    use rawr_compress::Compression;
    use std::collections::HashMap;

    fn minimal_config() -> Config {
        Config {
            library: LibraryConfig {
                targets: LibraryTargets {
                    import: "local".to_string(),
                    export: "local".to_string(),
                    trash: None,
                },
                cache: RelativePathBuf::from("/tmp/.rawr-test.db"),
                compression: Compression::default(),
                path_templates: PathTemplates {
                    import: "{{ fandom }}/{{ title }}.html".to_string(),
                    export: "".to_string(),
                },
                styles: vec![],
            },
            targets: HashMap::from([(
                "local".to_string(),
                TargetConfig::Local {
                    directory: RelativePathBuf::from("/tmp/test"),
                    auto_create: true,
                },
            )]),
            fandoms: FandomConfig::default(),
        }
    }

    #[test]
    fn valid_config_has_no_errors() {
        let config = minimal_config();
        let errors = config.validate();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn invalid_library_target() {
        let mut config = minimal_config();
        config.library.targets.import = "nonexistent".to_string();
        let errors = config.validate();
        eprintln!("{errors:?}");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].path.contains("library.targets.import"));
        assert!(errors[0].message.contains("nonexistent"));
    }

    #[test]
    fn test_no_create_warning() {
        let mut config = minimal_config();
        let Some(TargetConfig::Local { auto_create, .. }) = config.targets.get_mut("local") else {
            panic!("Unexpected target config");
        };
        *auto_create = false;
        let errors = config.validate();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].severity, ViolationSeverity::Warning);
        assert_eq!(errors[0].path, "targets.local.directory");
        assert!(errors[0].message.contains("auto_create is disabled"));
    }

    #[test]
    fn empty_s3_fields() {
        let mut config = minimal_config();
        config.targets.insert(
            "s3".to_string(),
            TargetConfig::S3 {
                bucket: "".to_string(),
                // prefix: None,
                region: "".to_string(),
                endpoint: None,
                key_id: MaybeFile::new("key", None::<String>),
                key_secret: MaybeFile::new("", None::<String>),
            },
        );
        let errors = config.validate();
        assert_eq!(errors.len(), 3); // bucket, region, secret_access_key
    }
}
