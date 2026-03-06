//! Auto-configuration for missing config values.
//!
//! When a user provides a minimal configuration, this module fills in values
//! that can be sensibly derived from what *is* present. [`Configurator`] wraps
//! a [`Figment`] and implements [`Provider`], so the loader can slot it into
//! the figment chain transparently.
//!
//! Three defaults are applied, in order:
//!
//! 1. **Import target** — if exactly one [`target`](crate::models::TargetConfig)
//!    is defined and `library.targets.import` is unset, the lone target is used.
//!    When multiple targets exist the choice is ambiguous, so nothing is inferred.
//! 2. **Export target** — if `library.targets.export` is unset, it mirrors the
//!    resolved import target (whether user-specified or auto-configured above).
//! 3. **Cache database** — if `library.cache` is unset, it defaults to the
//!    platform-specific data directory (via [`directories::ProjectDirs`]).

use crate::APP_NAME;
use directories::ProjectDirs;
use figment::value::{Dict, Map, Tag, Value};
use figment::{Error, Figment, Metadata, Profile, Provider};
use std::collections::BTreeSet;

type ProviderData = Map<Profile, Dict>;

/// A [`Provider`] decorator that fills in missing configuration values
/// with defaults derived from the values that *are* present.
///
/// Wraps an existing [`Figment`] and intercepts [`Provider::data`] to mutate
/// the raw provider data before it is extracted into a [`Config`](crate::models::Config).
pub(crate) struct Configurator {
    figment: Figment,
}
impl Provider for Configurator {
    fn metadata(&self) -> Metadata {
        Metadata::named("Config with auto-configured defaults")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, Error> {
        // Get the raw data from the underlying figment
        let mut data = self.figment.data()?;
        Self::autoconfigure_library_target_import(&mut data);
        Self::autoconfigure_library_target_export(&mut data);
        Self::autoconfigure_cache_database(&mut data);
        Ok(data)
    }
}
impl Configurator {
    /// Wrap a [`Figment`] so its data will be auto-configured on extraction.
    pub(crate) fn from(figment: Figment) -> Self {
        Self { figment }
    }

    /// Sets `library.targets.import` to the sole target name when exactly one
    /// [`TargetConfig`](crate::models::TargetConfig) is defined and the import
    /// target is not already specified. Does nothing when multiple targets exist
    /// because the intended target would be ambiguous.
    fn autoconfigure_library_target_import(data: &mut ProviderData) {
        let mut target_names = BTreeSet::new();
        for dict in data.values_mut() {
            if let Some(Value::Dict(_, targets)) = dict.get("targets") {
                target_names.extend(targets.keys().cloned());
            };
        }
        if target_names.len() != 1 {
            // If there isn't exactly one target specified then the
            // correct target to use is ambiguous.
            return;
        }
        let is_import_target_specified = data.values_mut().any(|dict| {
            get_or_insert_dict(dict, "library")
                .and_then(|(_, library)| get_or_insert_dict(library, "targets"))
                .map(|(_, targets)| is_value_set(targets, "import"))
                .unwrap_or(false)
        });
        if is_import_target_specified {
            return;
        }
        // Safety: target_names is guaranteed to contain a value.
        let singular_target_name = target_names.pop_last().unwrap();
        let default_profile = data.entry(Profile::Default).or_insert_with(Dict::new);
        if let Some((_, library)) = get_or_insert_dict(default_profile, "library")
            && let Some((targets_tag, targets)) = get_or_insert_dict(library, "targets")
        {
            tracing::debug!(setting = "library.targets.import", value = &singular_target_name, "Auto-configuring");
            targets.insert("import".into(), Value::String(targets_tag, singular_target_name));
        }
    }

    /// Sets `library.targets.export` to match the resolved import target when
    /// the export target is not already specified.
    fn autoconfigure_library_target_export(data: &mut ProviderData) {
        let is_export_target_specified = data.values_mut().any(|dict| {
            get_or_insert_dict(dict, "library")
                .and_then(|(_, library)| get_or_insert_dict(library, "targets"))
                .map(|(_, targets)| is_value_set(targets, "export"))
                .unwrap_or(false)
        });
        if is_export_target_specified {
            return;
        }
        let import_value = data
            .values_mut()
            .fold(None, |carry, dict| {
                get_or_insert_dict(dict, "library")
                    .and_then(|(_, library)| get_or_insert_dict(library, "targets"))
                    .and_then(|(_, targets)| match targets.get("import") {
                        Some(Value::String(_, s)) if !s.is_empty() => Some(s),
                        _ => carry,
                    })
            })
            .cloned();
        let default_profile = data.entry(Profile::Default).or_insert_with(Dict::new);
        if let Some(import_value) = import_value
            && let Some((_, library)) = get_or_insert_dict(default_profile, "library")
            && let Some((targets_tag, targets)) = get_or_insert_dict(library, "targets")
        {
            tracing::debug!(setting = "library.targets.export", value = &import_value, "Auto-configuring");
            targets.insert("export".into(), Value::String(targets_tag, import_value));
        }
    }

    /// Sets `library.cache` to the platform-specific data directory
    /// (e.g. `~/.local/share/rawr/cache.db` on Linux) when no cache path is
    /// configured. Uses [`directories::ProjectDirs`] for platform resolution.
    fn autoconfigure_cache_database(data: &mut ProviderData) {
        // Do any of the profiles contain a cache database path?
        let is_database_specified = data.values_mut().any(|dict| {
            get_or_insert_dict(dict, "library").map(|(_, library)| is_value_set(library, "cache")).unwrap_or(false)
        });
        if is_database_specified {
            return;
        }
        let default_profile = data.entry(Profile::Default).or_insert_with(Dict::new);
        if let Some((library_tag, library)) = get_or_insert_dict(default_profile, "library")
            && let Some(dirs) = ProjectDirs::from("", "", APP_NAME)
        {
            let database = dirs.data_dir().join("cache.db").to_string_lossy().to_string();
            tracing::debug!(setting = "library.cache", value = &database, "Auto-configuring");
            library.insert("cache".into(), Value::String(library_tag, database));
        }
    }
}

/// Navigates into a nested [`Dict`] by `key`, inserting an empty dict if absent.
/// Returns `None` if the key exists but holds a non-dict value.
fn get_or_insert_dict(dict: &mut Dict, key: impl Into<String>) -> Option<(Tag, &mut Dict)> {
    match dict.entry(key.into()).or_insert_with(|| Value::Dict(Tag::Default, Dict::new())) {
        Value::Dict(tag, child) => Some((*tag, child)),
        _ => None,
    }
}

/// Whether `key` exists in `dict` and holds a non-empty value.
/// An empty string is treated as unset, matching figment's behavior for
/// environment-variable overrides where `RAWR__X=""` means "clear this field."
fn is_value_set(dict: &Dict, key: impl AsRef<str>) -> bool {
    match dict.get(key.as_ref()) {
        Some(Value::String(_, s)) if s.is_empty() => false,
        Some(_) => true,
        None => false,
    }
}
