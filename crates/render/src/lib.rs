//! Document rendering for AO3 HTML works.
//!
//! This crate provides format-specific renderers for converting AO3 HTML
//! documents into other formats. Each format lives behind its own feature flag:
//!
//! - **`pdf`** — Chrome/Chromium-based HTML-to-PDF rendering ([`pdf`] module)
//! - **`epub`** — EPUB archive generation ([`epub`] module)
//!
//! The shared [`StyleConfig`] builder and [`CssVariables`] types are always
//! available for assembling CSS stylesheets used by any renderer.
//!
//! # Usage
//!
//! ```no_run
//! # #[cfg(feature = "pdf")]
//! # {
//! use rawr_render::{PdfRenderer, StyleConfig};
//! # use rawr_render::error::Result;
//!
//! # fn example() -> Result<()> {
//! let renderer: PdfRenderer = StyleConfig::new()
//!     .with_builtin("book.css")?
//!     .try_into()?;
//!
//! let output = renderer.render_slice(b"<html><head></head><body>Hello</body></html>", None)?;
//! println!("PDF at: {}", output.path().display());
//! # Ok(())
//! # }
//! # }
//! ```

#[cfg(feature = "epub")]
pub mod epub;
pub mod error;
#[cfg(feature = "pdf")]
pub mod pdf;
mod style;

#[cfg(feature = "epub")]
pub use crate::epub::EpubRenderer;
#[cfg(feature = "pdf")]
pub use crate::pdf::PdfRenderer;
pub use crate::style::{StyleConfig, variables::CssVariables};

/// Handle to a temporary file that is deleted when dropped.
///
/// Render operations that don't specify an output path return a temporary
/// file wrapping this type. Hold onto the value for as long as you need the output.
pub type TempFile = tempfile::NamedTempFile;
