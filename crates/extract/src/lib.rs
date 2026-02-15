mod compare;
mod consts;
pub mod error;
mod extract;
pub mod models;
mod truncate;

use crate::error::Result;
pub use crate::extract::{Datalist, Extractor, Stats, is_valid};
use crate::models::Metadata;
pub use crate::truncate::{ESTIMATED_HEADER_SIZE_BYTES, safe_html_truncate};

/// Easy, top-level entrypoint for the extraction of [`Metadata`] from HTML strings.
/// - Automatically truncates large HTML documents, and
/// - Validates the document as part of the extraction.
///
/// See [`Extractor`] for more details.
pub fn extract(html: &str) -> Result<Metadata> {
    // TODO: Perform hashing of input HTML, and return Version instead of Metadata.
    Extractor::from_long_html(html).metadata()
}
