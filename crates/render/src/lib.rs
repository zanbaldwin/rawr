//! Chrome/Chromium-based HTML-to-PDF rendering.
//!
//! This crate converts HTML documents into PDFs by driving a locally installed
//! Chrome or Chromium browser in headless mode. CSS stylesheets (both built-in
//! and user-provided) are injected into the HTML before rendering, and optional
//! CSS custom properties can be set via [`CssVariables`].
//!
//! # Usage
//!
//! ```no_run
//! use rawr_render::{Renderer, StyleConfig};
//! # use rawr_render::error::Result;
//!
//! # fn example() -> Result<()> {
//! let renderer: Renderer = StyleConfig::new()
//!     .with_builtin("book.css")?
//!     .try_into()?;
//!
//! let output = renderer.render_slice(b"<html><head></head><body>Hello</body></html>", None)?;
//! println!("PDF at: {}", output.path().display());
//! # Ok(())
//! # }
//! ```

mod chrome;
pub mod error;
mod render;
mod style;

use crate::chrome::Chrome;
use crate::error::{Error, Result};
pub use crate::render::Output;
pub use crate::style::{StyleConfig, variables::CssVariables};

/// Handle to a temporary file that is deleted when dropped.
///
/// Render operations that don't specify an output path return an [`Output::Temporary`]
/// wrapping this type. Hold onto the [`Output`] value for as long as you need the PDF.
pub type TempFile = tempfile::NamedTempFile;

/// An HTML-to-PDF renderer backed by a discovered Chrome/Chromium installation.
///
/// Construction auto-discovers Chrome on the system (direct binary or Flatpak)
/// and captures the [`StyleConfig`] to inject into every rendered document.
/// See the [render methods](Renderer::render) for producing PDFs.
pub struct Renderer {
    chrome: Chrome,
    styles: StyleConfig,
}
impl Renderer {
    /// Creates a new renderer with the given style configuration.
    ///
    /// Discovers a Chrome/Chromium executable on the system at construction
    /// time. Returns [`ErrorKind::ChromeNotFound`](error::ErrorKind::ChromeNotFound)
    /// if no suitable browser is available.
    pub fn new(styles: StyleConfig) -> Result<Self> {
        styles.try_into()
    }
}
impl TryFrom<StyleConfig> for Renderer {
    type Error = Error;
    fn try_from(styles: StyleConfig) -> std::result::Result<Self, Self::Error> {
        Ok(Self { chrome: Chrome::discover()?, styles })
    }
}
