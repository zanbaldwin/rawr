//! Style-list resolution.
//!
//! The `builtin:` prefix convention for `library.styles` entries is
//! implemented here and nowhere else: `builtin:epub.css` resolves to a
//! bundled stylesheet, anything else is read from disk (eagerly, so a
//! bad path fails at startup rather than mid-render).

use crate::error::{ErrorKind, Result};
use exn::ResultExt;
use rawr_config::Config;
use rawr_config::models::StyleFormat;
use rawr_render::StyleConfig;

/// The stylesheets for an output format: its `library.styles` list (see
/// [`Styles::for_format`](rawr_config::models::Styles::for_format)). There
/// are no default stylesheets: with no list, the format gets none.
pub fn styles_for(config: &Config, format: StyleFormat) -> Result<StyleConfig> {
    style_config(config.library.styles.for_format(format).unwrap_or_default(), [])
}

/// Fold a raw `library.styles` list into a [`StyleConfig`].
///
/// When `styles` is empty, `fallback` is used instead — pass `[]` to
/// keep an empty list empty, or e.g. `["builtin:epub.css"]` so an
/// unconfigured library still renders sensibly.
pub fn style_config<'a>(styles: &[String], fallback: impl IntoIterator<Item = &'a str>) -> Result<StyleConfig> {
    let mut config = StyleConfig::new();
    if styles.is_empty() {
        for item in fallback {
            config = apply(config, item)?;
        }
    } else {
        for item in styles {
            config = apply(config, item)?;
        }
    }
    Ok(config)
}

fn apply(config: StyleConfig, item: &str) -> Result<StyleConfig> {
    if let Some(name) = item.strip_prefix("builtin:") {
        config.with_builtin(name)
    } else {
        config.with_file(item)
    }
    .or_raise(|| ErrorKind::Style)
}
