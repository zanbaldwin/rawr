//! Embedded assets for rendering.
//!
//! This module provides access to CSS styles and other assets that are
//! embedded into the binary at compile time using [`rust-embed`](rust_embed).

use crate::error::{ErrorKind, Result};
use exn::OptionExt;
use rust_embed::Embed;
use std::borrow::Cow;

#[derive(Embed)]
#[folder = "../../assets/styles/"]
pub struct Builtins;
impl Builtins {
    /// Get the CSS content for a builtin style by name.
    pub fn load(name: impl AsRef<str>) -> Result<Cow<'static, [u8]>> {
        Self::get(name.as_ref()).map(|f| f.data).ok_or_raise(|| ErrorKind::AssetNotFound(Self::identifier(name)))
    }

    /// List all available builtin style names.
    pub fn list() -> Vec<Cow<'static, str>> {
        Self::iter().filter(|f| f.ends_with(".css")).collect()
    }

    pub fn exists(name: impl AsRef<str>) -> bool {
        Self::get(name.as_ref()).is_some()
    }

    pub(crate) fn identifier(name: impl AsRef<str>) -> String {
        format!("builtin:{}", name.as_ref().trim().trim_start_matches("builtin:"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_load_book_style() {
        let css = Builtins::load("book.css");
        assert!(css.is_ok());
        assert!(!css.unwrap().is_empty());
    }

    #[test]
    fn list_includes_book() {
        assert!(Builtins::exists("book.css"));
        let styles = Builtins::list();
        assert!(styles.iter().any(|s| s == "book.css"));
    }
}
