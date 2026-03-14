//! CSS style management for rendered documents.
//!
//! Styles are assembled through [`StyleConfig`]'s builder API, combining
//! compile-time embedded builtins (see [`StyleConfig::list_builtins`]) with
//! user-provided files or raw CSS content. All styles are read eagerly at
//! construction time so that missing files fail fast rather than at render time.

pub(crate) mod assets;
pub(crate) mod variables;

pub(crate) use self::variables::CssVariables;
use crate::error::{ErrorKind, Result};
use crate::style::assets::Builtins;
use exn::ResultExt;
use std::borrow::Cow;
use std::io::{Read, Result as IoResult, Write};
use std::{fs::File, path::Path};

enum Style {
    Builtin(String),
    // Since styles should be constructed once per invocation, we can read
    // file contents during construction. We'd have to load them at render
    // time anyway, so do here and fail fast.
    UserContent(String),
}
impl Style {
    fn write_all_to(&self, w: &mut impl Write) -> IoResult<usize> {
        let content = match self {
            // Infallible: business logic dictates that the builtin exists.
            Self::Builtin(name) => Builtins::load(name).expect("builtin validated at construction"),
            Self::UserContent(content) => Cow::Borrowed(content.as_bytes()),
        };
        w.write_all(&content)?;
        Ok(content.len())
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

    /// Write all style content to a writer, surrounding each block with delimiters.
    ///
    /// ```ignore
    /// # use std::io::Cursor;
    /// # let styles = StyleConfig::new();
    /// # let mut writer = Cursor::new(Vec::new());
    ///
    /// // No delimiters — raw CSS output:
    /// styles.write_all_to(&mut writer, None::<(&[u8], &[u8])>);
    ///
    /// // Both delimiters:
    /// styles.write_all_to(&mut writer, (b"<style>", b"</style>\n"));
    ///
    /// // Start delimiter only:
    /// styles.write_all_to(&mut writer, (b"<style>", None::<&[u8]>));
    ///
    /// // End delimiter only:
    /// styles.write_all_to(&mut writer, (None::<&[u8]>, Some(b"</style>\n")));
    /// ```
    pub fn write_all_to<S, E, B1, B2>(
        &self,
        w: &mut impl Write,
        delimiters: impl Into<Option<(S, E)>>,
    ) -> IoResult<usize>
    where
        B1: AsRef<[u8]>,
        B2: AsRef<[u8]>,
        S: Into<Option<B1>>,
        E: Into<Option<B2>>,
    {
        let delimiters = delimiters.into().map(|(start, end)| (start.into(), end.into()));
        for style in &self.styles {
            if let Some((Some(ref start), _)) = delimiters {
                w.write_all(start.as_ref())?;
            }
            style.write_all_to(w)?;
            if let Some((_, Some(ref end))) = delimiters {
                w.write_all(end.as_ref())?;
            }
        }
        Ok(self.styles.len())
    }

    #[cfg(feature = "pdf")]
    pub(crate) fn write_style_to(&self, w: &mut impl Write) -> IoResult<usize> {
        self.write_all_to(w, (b"<style>", b"</style>\n"))
    }
}
