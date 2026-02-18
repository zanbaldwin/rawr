//! CSS style management for rendered documents.
//!
//! Styles are assembled through [`StyleConfig`]'s builder API, combining
//! compile-time embedded builtins (see [`StyleConfig::list_builtins`]) with
//! user-provided files or raw CSS content. All styles are read eagerly at
//! construction time so that missing files fail fast rather than at render time.

mod assets;
pub(crate) mod variables;

pub(crate) use self::variables::CssVariables;
use crate::error::{ErrorKind, Result};
use crate::style::assets::Builtins;
use exn::ResultExt;
use std::borrow::Cow;
use std::{fs::File, path::Path};
use std::{io::Read, io::Write};

enum Style {
    Builtin(String),
    // Since styles should be constructed once per invocation, we can read
    // file contents during construction. We'd have to load them at render
    // time anyway, so do here and fail fast.
    UserContent(String),
}
impl Style {
    fn write_all_to(&self, w: &mut impl Write) -> std::io::Result<()> {
        let content = match self {
            // Infallible: business logic dictates that the builtin exists.
            Self::Builtin(name) => Builtins::load(name).expect("builting validated at construction"),
            Self::UserContent(content) => Cow::Borrowed(content.as_bytes()),
        };
        w.write_all(b"<style>")?;
        w.write_all(&content)?;
        w.write_all(b"</style>\n")
    }
}

/// An ordered collection of CSS stylesheets to inject into rendered documents.
///
/// Styles are applied in insertion order — later styles override earlier ones.
/// Use the builder methods to compose builtins, files, and raw CSS content.
///
/// # Example
///
/// ```no_run
/// use rawr_render::StyleConfig;
/// # use rawr_render::error::Result;
///
/// # fn get_styles() -> Result<StyleConfig> {
/// let styles = StyleConfig::new()
///     .with_builtin("book.css")?
///     .with_builtin("rmpp.css")?
///     .with_file("/path/to/custom.css")?;
/// # Ok(styles)
/// # }
/// ```
#[derive(Default)]
pub struct StyleConfig {
    styles: Vec<Style>,
}
impl StyleConfig {
    /// Creates an empty style configuration with no stylesheets.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the names of all embedded builtin stylesheets (e.g. `"book.css"`).
    pub fn list_builtins() -> Vec<Cow<'static, str>> {
        assets::Builtins::list()
    }

    /// Appends a builtin stylesheet by name.
    ///
    /// Returns [`ErrorKind::AssetNotFound`](crate::error::ErrorKind::AssetNotFound)
    /// if `name` does not match any embedded asset. Use [`list_builtins()`](Self::list_builtins)
    /// to discover available names.
    pub fn with_builtin(mut self, name: impl AsRef<str>) -> Result<Self> {
        let name = name.as_ref();
        if !Builtins::exists(name) {
            exn::bail!(ErrorKind::AssetNotFound(Builtins::identifier(name)));
        }
        self.styles.push(Style::Builtin(name.to_string()));
        Ok(self)
    }

    /// Appends a stylesheet read from a file on disk.
    ///
    /// The file is read immediately so that missing or unreadable files
    /// surface as errors during construction rather than at render time.
    pub fn with_file(mut self, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            exn::bail!(ErrorKind::AssetNotFound(path.display().to_string()));
        }
        let mut file = File::open(path).or_raise(|| ErrorKind::Io)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf).or_raise(|| ErrorKind::Io)?;
        self.styles.push(Style::UserContent(buf));
        Ok(self)
    }

    /// Appends raw CSS content as a stylesheet. This is infallible since no
    /// I/O is involved.
    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.styles.push(Style::UserContent(content.into()));
        self
    }

    pub(crate) fn write_all_to(&self, w: &mut impl Write) -> std::io::Result<usize> {
        for style in &self.styles {
            style.write_all_to(w)?;
        }
        Ok(self.styles.len())
    }
}
