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
//! 3. **Audio target** — if `library.targets.audio` is unset, it mirrors the
//!    resolved export target.
//! 4. **Audio template** — if `library.path_templates.audio` is unset, it is
//!    the export template, with `.audio` added when the audio target is the
//!    export target (so audiobooks never overwrite text exports).
//! 5. **Cache database** — if `library.cache` is unset, it defaults to the
//!    platform-specific data directory (via [`directories::ProjectDirs`]).

use crate::APP_NAME;
use crate::models::{DEFAULT_TEMPLATE_EXPORT, audio_template};
use directories::ProjectDirs;
use figment::value::{Dict, Map, Tag, Value};
use figment::{Error, Figment, Metadata, Profile, Provider};
use std::collections::BTreeSet;

type ProviderData = Map<Profile, Dict>;

/// A [`Provider`] decorator that fills in missing configuration values
/// with defaults derived from the values that *are* present.
///
/// Only emits the values it actually auto-configured, so that Figment
/// attributes missing-field errors to the original file provider rather
/// than to this decorator.
pub(crate) struct Configurator {
    data: ProviderData,
}
impl Provider for Configurator {
    fn metadata(&self) -> Metadata {
        Metadata::named("auto-configured defaults")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, Error> {
        Ok(self.data.clone())
    }
}
impl Configurator {
    /// Inspect the existing [`Figment`] and produce a provider containing
    /// only the values that can be auto-configured from what is present.
    pub(crate) fn from(figment: &Figment) -> Self {
        let source = figment.data().unwrap_or_default();
        let mut defaults: ProviderData = Map::new();
        let auto_import = Self::autoconfigure_library_target_import(&source, &mut defaults);
        let export = Self::autoconfigure_library_target_export(&source, auto_import.as_deref(), &mut defaults);
        let audio = Self::autoconfigure_library_target_audio(&source, export.as_deref(), &mut defaults);
        Self::autoconfigure_path_template_audio(&source, export.as_deref(), audio.as_deref(), &mut defaults);
        Self::autoconfigure_cache_database(&source, &mut defaults);
        Self { data: defaults }
    }

    /// Sets `library.targets.import` to the sole target name when exactly one
    /// [`TargetConfig`](crate::models::TargetConfig) is defined and the import
    /// target is not already specified. Does nothing when multiple targets exist
    /// because the intended target would be ambiguous.
    ///
    /// Returns the auto-configured import target name (if any) so that
    /// [`autoconfigure_library_target_export`](Self::autoconfigure_library_target_export)
    /// can use it without re-reading from `defaults`.
    fn autoconfigure_library_target_import(source: &ProviderData, defaults: &mut ProviderData) -> Option<String> {
        let mut target_names = BTreeSet::new();
        for dict in source.values() {
            if let Some((_, targets)) = get_dict(dict, "targets") {
                target_names.extend(targets.keys().cloned());
            };
        }
        if target_names.len() != 1 {
            // If there isn't exactly one target specified then the
            // correct target to use is ambiguous.
            return None;
        }
        let is_import_target_specified = source.values().any(|dict| {
            get_dict(dict, "library")
                .and_then(|(_, library)| get_dict(library, "targets"))
                .map(|(_, targets)| is_value_set(targets, "import"))
                .unwrap_or(false)
        });
        if is_import_target_specified {
            return None;
        }
        // Safety: target_names is guaranteed to contain a value.
        let singular_target_name = target_names.pop_last().unwrap();
        let default_profile = defaults.entry(Profile::Default).or_default();
        if let Some((_, library)) = get_or_insert_dict(default_profile, "library")
            && let Some((targets_tag, targets)) = get_or_insert_dict(library, "targets")
        {
            tracing::debug!(setting = "library.targets.import", value = &singular_target_name, "Auto-configuring");
            targets.insert("import".into(), Value::String(targets_tag, singular_target_name.clone()));
        }
        Some(singular_target_name)
    }

    /// Sets `library.targets.export` to match the resolved import target when
    /// the export target is not already specified. The import target is resolved
    /// from the source data first, falling back to `auto_import` if the import
    /// target was itself auto-configured in the previous step.
    ///
    /// Returns the resolved export target name (specified or auto-configured).
    fn autoconfigure_library_target_export(
        source: &ProviderData,
        auto_import: Option<&str>,
        defaults: &mut ProviderData,
    ) -> Option<String> {
        if let Some(export) = library_value(source, "targets", "export") {
            return Some(export);
        }
        let import_from_source = source.values().fold(None, |carry, dict| {
            get_dict(dict, "library").and_then(|(_, library)| get_dict(library, "targets")).and_then(|(_, targets)| {
                match targets.get("import") {
                    Some(Value::String(_, s)) if !s.is_empty() => Some(s.as_str()),
                    _ => carry,
                }
            })
        });
        let import_value = import_from_source.or(auto_import);
        let default_profile = defaults.entry(Profile::Default).or_default();
        if let Some(import_value) = import_value
            && let Some((_, library)) = get_or_insert_dict(default_profile, "library")
            && let Some((targets_tag, targets)) = get_or_insert_dict(library, "targets")
        {
            tracing::debug!(setting = "library.targets.export", value = import_value, "Auto-configuring");
            targets.insert("export".into(), Value::String(targets_tag, import_value.to_owned()));
        }
        import_value.map(str::to_owned)
    }

    /// Sets `library.targets.audio` to the resolved export target when the
    /// audio target is not already specified.
    ///
    /// Returns the resolved audio target name (specified or auto-configured).
    fn autoconfigure_library_target_audio(
        source: &ProviderData,
        export: Option<&str>,
        defaults: &mut ProviderData,
    ) -> Option<String> {
        if let Some(audio) = library_value(source, "targets", "audio") {
            return Some(audio);
        }
        let export = export?;
        let default_profile = defaults.entry(Profile::Default).or_default();
        if let Some((_, library)) = get_or_insert_dict(default_profile, "library")
            && let Some((targets_tag, targets)) = get_or_insert_dict(library, "targets")
        {
            tracing::debug!(setting = "library.targets.audio", value = export, "Auto-configuring");
            targets.insert("audio".into(), Value::String(targets_tag, export.to_owned()));
        }
        Some(export.to_owned())
    }

    /// Sets `library.path_templates.audio` from the export template when it is
    /// not already specified: the export template plus `.audio` when the audio
    /// target is the export target, or the export template unchanged when the
    /// audio target is separate.
    fn autoconfigure_path_template_audio(
        source: &ProviderData,
        export: Option<&str>,
        audio: Option<&str>,
        defaults: &mut ProviderData,
    ) {
        if library_value(source, "path_templates", "audio").is_some() {
            return;
        }
        let export_template = library_value(source, "path_templates", "export");
        let export_template = export_template.as_deref().unwrap_or(DEFAULT_TEMPLATE_EXPORT);
        let template = audio_template(export_template, export == audio);
        let default_profile = defaults.entry(Profile::Default).or_default();
        if let Some((_, library)) = get_or_insert_dict(default_profile, "library")
            && let Some((templates_tag, templates)) = get_or_insert_dict(library, "path_templates")
        {
            tracing::debug!(setting = "library.path_templates.audio", value = &template, "Auto-configuring");
            templates.insert("audio".into(), Value::String(templates_tag, template));
        }
    }

    /// Sets `library.cache` to the platform-specific data directory
    /// (e.g. `~/.local/share/rawr/cache.db` on Linux) when no cache path is
    /// configured. Uses [`directories::ProjectDirs`] for platform resolution.
    fn autoconfigure_cache_database(source: &ProviderData, defaults: &mut ProviderData) {
        // Do any of the profiles contain a cache database path?
        let is_database_specified = source
            .values()
            .any(|dict| get_dict(dict, "library").map(|(_, library)| is_value_set(library, "cache")).unwrap_or(false));
        if is_database_specified {
            return;
        }
        let default_profile = defaults.entry(Profile::Default).or_default();
        if let Some((library_tag, library)) = get_or_insert_dict(default_profile, "library")
            && let Some(dirs) = ProjectDirs::from("", "", APP_NAME)
        {
            let database = dirs.data_dir().join("cache.db").to_string_lossy().to_string();
            tracing::debug!(setting = "library.cache", value = &database, "Auto-configuring");
            library.insert("cache".into(), Value::String(library_tag, database));
        }
    }
}

/// The last non-empty string at `library.{table}.{key}` across profiles.
fn library_value(source: &ProviderData, table: &str, key: &str) -> Option<String> {
    source.values().fold(None, |carry, dict| {
        get_dict(dict, "library")
            .and_then(|(_, library)| get_dict(library, table))
            .and_then(|(_, table)| match table.get(key) {
                Some(Value::String(_, s)) if !s.is_empty() => Some(s.clone()),
                _ => None,
            })
            .or(carry)
    })
}

/// Navigates into a nested [`Dict`] by `key`, inserting an empty dict if absent.
/// Returns `None` if the key exists but holds a non-dict value.
fn get_or_insert_dict(dict: &mut Dict, key: impl Into<String>) -> Option<(Tag, &mut Dict)> {
    match dict.entry(key.into()).or_insert_with(|| Value::Dict(Tag::Default, Dict::new())) {
        Value::Dict(tag, child) => Some((*tag, child)),
        _ => None,
    }
}

/// Read-only counterpart to [`get_or_insert_dict`]. Returns the nested dict
/// at `key` without inserting anything if the key is absent.
fn get_dict<'a>(dict: &'a Dict, key: &str) -> Option<(Tag, &'a Dict)> {
    match dict.get(key) {
        Some(Value::Dict(tag, child)) => Some((*tag, child)),
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
