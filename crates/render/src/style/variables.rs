//! CSS custom properties (variables) for rendered documents.
//!
//! [`CssVariables`] is rendered as a `<style>` block setting `:root` custom
//! properties prefixed with `--meta-`. These are injected alongside stylesheets
//! so that CSS rules can reference work metadata (title, word count, etc.)
//! without template preprocessing.

#[cfg(feature = "metadata")]
use rawr_extract::models::Metadata;
use rslug::slugify;
use std::collections::HashMap;
use std::fmt::{Display, Formatter, Result as FmtResult};

/// A set of CSS custom properties injected as `:root` variables.
///
/// Each entry becomes `--meta-{key}: "{value}"` in a `<style>` block.
/// Values are escaped per the [W3C CSS string token grammar][spec].
///
/// [spec]: https://www.w3.org/TR/css-syntax-3/#consume-string-token
pub struct CssVariables {
    variables: HashMap<String, String>,
}

impl CssVariables {
    /// Creates a new set of CSS variables from any map-like type.
    pub fn new(map: impl Into<HashMap<String, String>>) -> Self {
        Self { variables: map.into() }
    }
}
impl<K: Into<String>, V: Into<String>> FromIterator<(K, V)> for CssVariables {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let variables = iter.into_iter().map(|(k, v)| (k.into(), v.into())).collect();
        Self { variables }
    }
}
/// Converts work [`Metadata`] into CSS variables for use in stylesheets.
///
/// Populates keys like `work-id`, `words`, `rating`, `published`, etc.
/// Numeric values are formatted with comma separators for display.
#[cfg(feature = "metadata")]
impl From<&Metadata> for CssVariables {
    fn from(m: &Metadata) -> Self {
        /// Format a number with comma separators: 21837 → "21,837"
        fn human_number(n: u64) -> String {
            let s = n.to_string();
            let mut result = String::new();
            for (i, c) in s.chars().rev().enumerate() {
                if i > 0 && i % 3 == 0 {
                    result.insert(0, ',');
                }
                result.insert(0, c);
            }
            result
        }
        let variables = [
            ("work-id", m.work_id.to_string()),
            ("summary", m.summary.as_deref().unwrap_or_default().to_string()),
            ("words", human_number(m.words)),
            ("chapters-written", m.chapters.written.to_string()),
            ("chapters-total", m.chapters.total.map_or("?".into(), |t| t.to_string())),
            ("rating", m.rating.map_or_else(String::new, |r| r.as_str().into())),
            ("published", m.published.to_string()),
            ("updated", m.last_modified.to_string()),
        ];
        variables.into_iter().collect()
    }
}
impl Display for CssVariables {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "<style>\n:root {{")?;
        for (key, value) in self.variables.iter() {
            writeln!(f, "    --meta-{}: \"{}\";", slugify!(key), css_escape_string(value))?;
        }
        write!(f, "}}\n</style>")
    }
}

/// https://www.w3.org/TR/css-syntax-3/#consume-string-token
fn css_escape_string(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\a ")
        .replace('\r', "\\d ")
        .replace('\x0C', "\\c ")
        .replace('\0', "\\fffd ")
}
