mod assets;
mod variables;

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
            // Safety: business logic dictates that the builtin exists.
            Self::Builtin(name) => Builtins::load(name).unwrap(),
            Self::UserContent(content) => Cow::Borrowed(content.as_bytes()),
        };
        w.write_all(b"<style>")?;
        w.write_all(&content)?;
        w.write_all(b"</style>\n")
    }
}

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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn list_builtins() -> Vec<Cow<'static, str>> {
        assets::Builtins::list()
    }

    pub fn with_builtin(mut self, name: impl AsRef<str>) -> Result<Self> {
        let name = name.as_ref();
        if !Builtins::exists(name) {
            exn::bail!(ErrorKind::AssetNotFound(Builtins::identifier(name)));
        }
        self.styles.push(Style::Builtin(name.to_string()));
        Ok(self)
    }

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
