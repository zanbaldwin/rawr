//! Chrome/Chromium-based HTML-to-PDF rendering.
//!
//! Converts HTML documents into PDFs by driving a locally installed
//! Chrome or Chromium browser in headless mode. CSS stylesheets are
//! injected into the HTML before rendering. Requires the `pdf` feature.

mod chrome;
mod render;

use self::chrome::Chrome;
use crate::error::{Error, Result};
use crate::style::StyleConfig;

/// An HTML-to-PDF renderer backed by a discovered Chrome/Chromium installation.
///
/// Construction auto-discovers Chrome on the system (direct binary or Flatpak)
/// and captures the [`StyleConfig`] to inject into every rendered document.
/// See the [render methods](PdfRenderer::render) for producing PDFs.
pub struct PdfRenderer {
    chrome: Chrome,
    styles: StyleConfig,
}

impl PdfRenderer {
    /// Creates a new renderer with the given style configuration.
    ///
    /// Discovers a Chrome/Chromium executable on the system at construction
    /// time. Returns [`ErrorKind::ChromeNotFound`](crate::error::ErrorKind::ChromeNotFound)
    /// if no suitable browser is available.
    pub fn new(styles: StyleConfig) -> Result<Self> {
        styles.try_into()
    }
}

impl TryFrom<StyleConfig> for PdfRenderer {
    type Error = Error;
    fn try_from(styles: StyleConfig) -> std::result::Result<Self, Self::Error> {
        Ok(Self { chrome: Chrome::discover()?, styles })
    }
}
