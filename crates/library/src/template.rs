//! Path templating for library file organization.
//!
//! Converts [`Version`] metadata into deterministic filesystem paths using
//! user-configured [upon] templates. The template syntax follows upon's
//! Mustache-like conventions (`{{ variable }}`, `{{ value|formatter }}`),
//! extended with library-specific formatters and functions:
//!
//! - **`slug`** — Converts strings to URL-safe slugs, stripping quotation marks
//!   first to avoid artifacts like leading/trailing hyphens.
//! - **`truncate`** — Truncates strings to a maximum byte length at a character
//!   boundary, usable as either `truncate(value, n)` or `{{ value|truncate: n }}`.
//!
//! # Template Variables
//!
//! | Variable            | Type              | Description                              |
//! |---------------------|-------------------|------------------------------------------|
//! | `work`              | `String`          | The AO3 work ID                          |
//! | `title`             | `String`          | Work title                               |
//! | `rating`            | `Option<String>`  | Short rating code (e.g. `"G"`, `"T"`)    |
//! | `words`             | `u64`             | Word count                               |
//! | `chapters.written`  | `u64`             | Number of posted chapters                 |
//! | `chapters.total`    | `Option<u64>`     | Planned total chapters                    |
//! | `fandom`            | `String`          | Alphabetically-first fandom name          |
//! | `series.id`         | `Option<u64>`     | ID of the lowest-ID series                |
//! | `series.name`       | `Option<String>`  | Name of that series                       |
//! | `series.position`   | `Option<u64>`     | Position within that series               |
//! | `hash`              | `String`          | Zero-padded 8-hex-digit CRC32 of content  |
//!
//! # Example
//!
//! ```
//! # fn main() {
//! use rawr_library::PathGenerator;
//! # use rawr_extract::models::*;
//! # use std::str::FromStr;
//! # use time::{Date, Month, UtcDateTime};
//! # let version = Version {
//! #     hash: String::new(), length: 0, crc32: 0,
//! #     metadata: Metadata {
//! #         work_id: 12345, title: "My Story".into(), authors: vec![],
//! #         fandoms: vec![Fandom { name: "Marvel".into() }],
//! #         rating: Some(Rating::TeenAndUp), warnings: vec![], tags: vec![],
//! #         summary: None, language: Language::from_str("English").unwrap(),
//! #         chapters: Chapters { written: 1, total: None },
//! #         words: 5000,
//! #         published: Date::from_calendar_date(2024, Month::January, 1).unwrap(),
//! #         last_modified: Date::from_calendar_date(2024, Month::January, 1).unwrap(),
//! #         series: vec![],
//! #     },
//! #     extracted_at: UtcDateTime::now(),
//! # };
//!
//! let generator: PathGenerator = "{{ fandom|slug }}/{{ work }}-{{ title|slug }}".parse().unwrap();
//! let path = generator.generate(&version).unwrap();
//! assert_eq!(path, "marvel/12345-my-story");
//! # }
//! ```

use crate::error::{Error, ErrorKind, Result};
use exn::{OptionExt, ResultExt};
use rawr_compress::Compression;
use rawr_extract::models::Version;
use rawr_storage::validate_path;
use std::str::FromStr;
use tracing::instrument;
use upon::{Engine, Template};

/// Generates deterministic filesystem paths from [`Version`] metadata and a
/// user-defined template string.
///
/// Constructed via [`FromStr`], which compiles the template eagerly so that
/// syntax errors surface at creation time rather than at render time. The
/// compiled template is reusable across many [`generate`](Self::generate) calls.
///
/// Generated paths are normalized (trimmed, deduplicated separators) and
/// validated by [`rawr_storage::validate_path`] to prevent directory traversal.
pub struct PathGenerator {
    engine: Engine<'static>,
    template: Template<'static>,
    // TODO when `rawr-config` is complete
    // config: Option<FandomConfig>,
}
impl FromStr for PathGenerator {
    type Err = Error;

    /// Compiles the given template string into a reusable [`PathGenerator`].
    ///
    /// Registers the `slug` formatter and `truncate` function before compiling,
    /// so both are available in the template. Returns [`ErrorKind::Template`] if
    /// the template syntax is invalid.
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let mut engine = Engine::new();
        addons::configure(&mut engine);
        // Compile the template early so we can fail-fast in construction.
        let template = engine.compile(s.to_string()).or_raise(|| ErrorKind::Template)?;
        Ok(Self { engine, template })
    }
}
impl PathGenerator {
    // TODO when `rawr-config` is complete
    // pub fn with_config(mut self config: impl Into<Option<FandomConfig>>) -> Self {
    //     self.config = config.into();
    //     self
    // }

    // pub fn new(template: impl AsRef<str>, config: impl Into<Option<FandomConfig>>) -> Result<Self> {
    //     template.as_ref().parse()?.with_config(config)
    // }

    /// Renders the template against the given [`Version`]'s metadata, returning
    /// the normalized path without any file extension.
    ///
    /// The resulting path is trimmed, segment-wise normalized, and validated to
    /// ensure it stays within the library root (no directory traversal).
    #[instrument(skip_all, fields(work_id = version.as_ref().metadata.work_id))]
    pub fn generate(&self, version: impl AsRef<Version>) -> Result<String> {
        let path = self
            .template
            .render(&self.engine, Self::parameters(version.as_ref()))
            .to_string()
            .or_raise(|| ErrorKind::Template)?;
        Self::normalize(path)
    }

    /// Renders the template and appends a file extension and optional compression suffix.
    ///
    /// The extension is dot-separated and trimmed of leading/trailing dots, so both
    /// `"html"` and `".html"` produce the same result. When `compression` is
    /// [`Compression::None`] (or `None`), no compression suffix is appended.
    ///
    /// ```text
    /// generate(…)           → "fandom/12345-story"
    /// generate_with_ext(…, "html", None)       → "fandom/12345-story.html"
    /// generate_with_ext(…, "html", Bzip2)      → "fandom/12345-story.html.bz2"
    /// ```
    pub fn generate_with_ext(
        &self,
        version: impl AsRef<Version>,
        ext: impl AsRef<str>,
        compression: impl Into<Option<Compression>>,
    ) -> Result<String> {
        let path = self.generate(version)?;
        let compression = compression.into().unwrap_or(Compression::None);
        Ok(format!("{path}.{}{}", ext.as_ref().trim().trim_matches('.'), compression.extension()))
    }

    /// Trims each path segment, joins them with `/`, then validates via
    /// [`rawr_storage::validate_path`].
    fn normalize(s: impl Into<String>) -> Result<String> {
        let path = s.into().trim().split('/').map(str::trim).collect::<Vec<_>>().join("/");
        validate_path(&path).or_raise(|| ErrorKind::Template).and_then(|p| {
            p.to_str().map(|p| p.to_string())
            // Infallible: input was String, so won't fail. Here for completeness.
            .ok_or_raise(|| ErrorKind::Template)
        })
    }

    /// Builds the [`upon::Value`] map exposed to the template engine.
    ///
    /// When a [`Version`] has multiple fandoms or series entries, only one is
    /// selected — the alphabetically-first fandom and the lowest-ID series —
    /// so that the generated path is deterministic regardless of ordering.
    fn parameters(version: &Version) -> upon::Value {
        // TODO rename and re-order fandoms according to preferences when `rawr-config` is complete
        let fandom = version
            .metadata
            .fandoms
            .iter()
            // The path should always be deterministic according to the version metadata.
            .min_by(|a, b| a.name.cmp(&b.name))
            .map(|f| f.name.clone());
        let series = version
            .metadata
            .series
            .iter()
            // Again, path should always be deterministic.
            .min_by_key(|s| s.id)
            .map(|series| {
                upon::value! {
                    id: series.id,
                    name: series.name.as_str(),
                    position: series.position,
                }
            });
        upon::value! {
            work: version.metadata.work_id.to_string(),
            title: &version.metadata.title,
            rating: version.metadata.rating.map(|r| r.as_short_str()),
            words: version.metadata.words,
            chapters: upon::value! {
                written: version.metadata.chapters.written,
                total: version.metadata.chapters.total,
            },
            fandom: fandom.unwrap_or_default(),
            series: series,
            hash: format!("{:08x}", version.crc32),
        }
    }
}

/// Custom [`upon`] extensions for path-safe string manipulation.
mod addons {
    use rslug::slugify;
    use std::fmt::Write;
    use upon::{Engine, Value, fmt as upon_fmt};

    /// Custom formatter that converts strings to URL-safe slugs.
    ///
    /// Strips quotation marks before slugifying to avoid awkward slug output
    /// like `"hello"` becoming `-hello-`.
    fn slug_formatter(f: &mut upon_fmt::Formatter<'_>, value: &Value) -> upon_fmt::Result {
        match value {
            Value::String(s) => {
                // Various quotation marks: '"''""„"`«»
                let marks = [
                    '\u{0027}', '\u{0022}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{201E}', '\u{201B}',
                    '\u{0060}', '\u{00AB}', '\u{00BB}', '\u{2039}', '\u{203A}',
                ];
                let stripped: String = s.chars().filter(|c| !marks.contains(c)).collect();
                write!(f, "{}", slugify!(&stripped))?
            },
            v => upon_fmt::default(f, v)?,
        };
        Ok(())
    }

    /// Truncates a string to a maximum byte length at a character boundary.
    ///
    /// This prevents cutting UTF-8 characters in the middle, which would produce
    /// invalid strings.
    fn truncate_to_char_boundary(s: &str, max_bytes: usize) -> String {
        s[..s.floor_char_boundary(max_bytes)].to_string()
    }

    /// Registers the `slug` formatter and `truncate` function on the given engine.
    pub(crate) fn configure(engine: &mut Engine<'_>) {
        engine.add_formatter("slug", slug_formatter);
        engine.add_function("truncate", truncate_to_char_boundary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawr_extract::models::{Chapters, Fandom, Language, Metadata, Rating, Version};
    use time::{Date, Month, UtcDateTime};

    fn make_test_version(work_id: u64, title: &str, fandom: &str) -> Version {
        Version {
            hash: "abc123".to_string(),
            length: 1000,
            crc32: 3_735_928_559,
            metadata: Metadata {
                work_id,
                title: title.to_string(),
                authors: vec![],
                fandoms: vec![Fandom { name: fandom.to_string() }],
                rating: Some(Rating::GeneralAudiences),
                warnings: vec![],
                tags: vec![],
                summary: None,
                language: Language::from_str("English").unwrap(),
                chapters: Chapters { written: 5, total: Some(10) },
                words: 25000,
                published: Date::from_calendar_date(2024, Month::January, 1).unwrap(),
                last_modified: Date::from_calendar_date(2024, Month::June, 15).unwrap(),
                series: vec![],
            },
            extracted_at: UtcDateTime::now(),
        }
    }

    #[test]
    fn test_generates_basic_path() {
        let template = "{{ fandom|slug }}/{{ work }}-{{ title|slug }}";
        let version = make_test_version(12345, "My Great Story", "Harry Potter");

        let generator: PathGenerator = template.parse().unwrap();
        let path = generator.generate(&version).unwrap();
        assert_eq!(path, "harry-potter/12345-my-great-story");
    }

    #[test]
    fn test_includes_hash_in_path() {
        let template = "{{ work }}-{{ hash }}";
        let version = make_test_version(12345, "Story", "Fandom");

        let generator: PathGenerator = template.parse().unwrap();
        let path = generator.generate(&version).unwrap();
        assert_eq!(path, "12345-deadbeef");
    }

    #[test]
    fn test_appends_html_extension() {
        let template = "{{ work }}";
        let version = make_test_version(123, "Title", "Fandom");

        let generator: PathGenerator = template.parse().unwrap();
        assert!(!generator.generate(&version).unwrap().ends_with(".html"));
        assert!(generator.generate_with_ext(&version, "html", None).unwrap().ends_with(".html"));
    }

    #[test]
    fn test_double_html_extension() {
        let template = "{{ work }}.html";
        let version = make_test_version(123, "Title", "Fandom");

        let generator: PathGenerator = template.parse().unwrap();
        let path = generator.generate(&version).unwrap();
        assert_eq!(path, "123.html");
        assert!(!path.ends_with(".html.html"));
        let path = generator.generate_with_ext(&version, "pdf", Compression::None).unwrap();
        assert_eq!(path, "123.html.pdf");
    }

    #[test]
    fn test_slug_strips_quotes() {
        let template = "{{ title|slug }}.html";
        let version = make_test_version(1, "\"Hello\" World's 'Test'", "Fandom");

        let generator: PathGenerator = template.parse().unwrap();
        assert_eq!(generator.generate(&version).unwrap(), "hello-worlds-test.html");
    }

    #[test]
    fn test_truncate_classic_function() {
        let template = "{{ truncate(title, 10)|slug }}";
        let version = make_test_version(1, "A Very Long Title Indeed", "Fandom");

        let generator: PathGenerator = template.parse().unwrap();
        let path = generator.generate(&version).unwrap();
        // "A Very Lon" truncated to 10 bytes, then slugified
        assert_eq!(path, "a-very-lon");
    }

    #[test]
    fn test_truncate_filter_function() {
        let template = "{{ title|truncate: 10|slug }}";
        let version = make_test_version(1, "A Very Long Title Indeed", "Fandom");

        let generator: PathGenerator = template.parse().unwrap();
        let path = generator.generate(&version).unwrap();
        // "A Very Lon" truncated to 10 bytes, then slugified
        assert_eq!(path, "a-very-lon");
    }

    #[test]
    fn test_generates_compressed_extension() {
        let template = "{{ work }}";
        let version = make_test_version(123, "Title", "Fandom");

        let generator: PathGenerator = template.parse().unwrap();
        let path = generator.generate_with_ext(&version, ".html", Compression::Bzip2).unwrap();
        assert_eq!(path, "123.html.bz2");
    }

    #[test]
    fn test_generates_pdf_extension() {
        let template = "{{ work }}";
        let version = make_test_version(123, "Title", "Fandom");

        let generator: PathGenerator = template.parse().unwrap();
        let path = generator.generate_with_ext(&version, "pdf", None).unwrap();
        assert_eq!(path, "123.pdf");
    }
}
