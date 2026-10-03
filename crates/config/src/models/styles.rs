use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use serde::de::{Deserializer, MapAccess, SeqAccess, Visitor};
use std::fmt::{Formatter, Result as FmtResult};

/// The output formats that can have their own stylesheet list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StyleFormat {
    Pdf,
    Epub,
    /// Narrated EPUB 3 audiobooks (an optional feature).
    Audiobook,
}

/// `library.styles`: stylesheet lists, by output format.
///
/// Each entry is a file path, or `builtin:` and the name of a bundled
/// stylesheet. Two shapes are accepted:
///
/// ```toml
/// [library]
/// styles = ["builtin:epub.css"]          # one list for every format
///
/// [library.styles]                       # or a list for each format
/// default = ["builtin:book.css"]
/// pdf = ["builtin:book.css", "print.css"]
/// epub = ["builtin:epub.css"]
/// audiobook = ["builtin:epub.css"]
/// ```
///
/// A format's list replaces `default`; it does not add to it. See
/// [`for_format`](Self::for_format) for the fallbacks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Styles {
    pub default: Option<Vec<String>>,
    pub pdf: Option<Vec<String>>,
    pub epub: Option<Vec<String>>,
    pub audiobook: Option<Vec<String>>,
}

impl Styles {
    /// One list for every format (the older `styles = [...]` shape).
    pub fn all(styles: Vec<String>) -> Self {
        Self { default: Some(styles), ..Self::default() }
    }

    /// The configured list for a format, or `None` when nothing applies.
    ///
    /// Fallbacks: `audiobook` → `epub` → `default`, `epub` → `default`,
    /// and `pdf` → `default`.
    pub fn for_format(&self, format: StyleFormat) -> Option<&[String]> {
        let own = match format {
            StyleFormat::Pdf => self.pdf.as_ref(),
            StyleFormat::Epub => self.epub.as_ref(),
            StyleFormat::Audiobook => self.audiobook.as_ref().or(self.epub.as_ref()),
        };
        own.or(self.default.as_ref()).map(Vec::as_slice)
    }
}

/// Accepts a list (for every format) or a table of lists (by format).
pub(crate) fn deserialize_styles<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Styles, D::Error> {
    deserializer.deserialize_any(StylesVisitor)
}

struct StylesVisitor;

impl<'de> Visitor<'de> for StylesVisitor {
    type Value = Styles;

    fn expecting(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("a list of stylesheets, or a table of lists with the keys default, pdf, epub and audiobook")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Styles, A::Error> {
        let mut styles = Vec::new();
        while let Some(style) = seq.next_element::<String>()? {
            styles.push(style);
        }
        Ok(Styles::all(styles))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Styles, A::Error> {
        Styles::deserialize(MapAccessDeserializer::new(map))
    }
}
