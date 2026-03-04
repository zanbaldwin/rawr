use figment::value::magic::RelativePathBuf;
use rawr_compress::Compression;
use serde::Deserialize;

/// Default Tera template for organizing imported works into the library.
///
/// Includes `{{ hash }}` to allow multiple versions of the same work to
/// coexist. Available variables: `fandom`, `series` (optional, with `.id`
/// and `.name`), `work`, `hash`, `title`.
pub const DEFAULT_TEMPLATE_IMPORT: &str = r#"
    {{ fandom|truncate: 255|slug }}/
    {% if series %}{{ series.id }}-{{ series.name|truncate: 230|slug }}/{% endif %}
    {{ work }}-{{ hash }}-{{ title|truncate: 220|slug }}
"#;

/// Default Tera template for exported PDFs.
///
/// Omits `{{ hash }}` since exports overwrite previous versions.
pub const DEFAULT_TEMPLATE_EXPORT: &str = r#"
    {{ fandom|truncate: 255|slug }}/
    {{ work }}-{{ title|truncate: 220|slug }}
"#;

/// Core library settings: where to store data, how to compress, and how
/// to organize files within storage targets.
#[derive(Debug, Deserialize)]
pub struct LibraryConfig {
    /// Path to the SQLite cache database. Relative paths are resolved
    /// against the config file's directory via figment's
    /// [`RelativePathBuf`].
    pub cache: RelativePathBuf,
    /// Compression algorithm applied during import/organize. Parsed from
    /// a string name (e.g. `"bzip2"`, `"zstd"`). Defaults to
    /// [`Compression::default`] (none).
    #[serde(default = "default_compression", deserialize_with = "deserialize_compression")]
    pub compression: Compression,
    /// Which named [`TargetConfig`](super::TargetConfig) entries to use
    /// for import, export, and trash.
    pub targets: LibraryTargets,
    /// Tera templates controlling file layout within storage targets.
    #[serde(default)]
    pub path_templates: PathTemplates,
    /// CSS stylesheet references applied during export. Supports
    /// `builtin:` prefixed names for bundled stylesheets.
    #[serde(default)]
    pub styles: Vec<String>,
}

/// Maps logical operations (import, export, trash) to named
/// [`TargetConfig`](super::TargetConfig) entries defined in the
/// top-level `targets` map.
#[derive(Debug, Deserialize)]
pub struct LibraryTargets {
    /// Storage target for imported works.
    pub import: String,
    /// Storage target for exported PDFs. Defaults to the import target
    /// via auto-configuration.
    pub export: String,
    /// Optional "recycle bin" target for files overwritten during
    /// organize. `None` means overwritten files are discarded.
    pub trash: Option<String>,
}

/// Tera templates controlling how files are organized within storage
/// targets.
#[derive(Debug, Deserialize)]
pub struct PathTemplates {
    /// Template for imported works. Defaults to
    /// [`DEFAULT_TEMPLATE_IMPORT`].
    #[serde(default = "default_template_import")]
    pub import: String,
    /// Template for exported PDFs. Defaults to
    /// [`DEFAULT_TEMPLATE_EXPORT`].
    #[serde(default = "default_template_export")]
    pub export: String,
}
impl Default for PathTemplates {
    fn default() -> Self {
        Self {
            import: DEFAULT_TEMPLATE_IMPORT.to_string(),
            export: DEFAULT_TEMPLATE_EXPORT.to_string(),
        }
    }
}

fn deserialize_compression<'de, D>(deserializer: D) -> Result<Compression, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<Compression>().map_err(serde::de::Error::custom)
}

fn default_compression() -> Compression {
    Compression::default()
}

fn default_template_import() -> String {
    DEFAULT_TEMPLATE_IMPORT.to_string()
}

fn default_template_export() -> String {
    DEFAULT_TEMPLATE_EXPORT.to_string()
}
