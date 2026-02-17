mod assets;
mod variables;

use crate::error::{ErrorKind, Result};
use crate::style::assets::Builtins;
use exn::ResultExt;
use std::borrow::Cow;
use std::fs::File;
use std::{io::Read, path::Path};

enum Style {
    Builtin(String),
    // Since styles should be constructed once per invocation, we can read
    // file contents during construction. We'd have to load them at render
    // time anyway, so do here and fail fast.
    UserContent(String),
}

/// # Example
///
/// ```no_run
/// let styles = StyleConfig::new()
///     .with_builtin("book.css")?
///     .with_builtin("rmpp.css")?
///     .with_file("/path/to/custom.css")?;
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
        if !Builtins::exists(&name) {
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
}
