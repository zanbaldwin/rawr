mod compare;
mod consts;
pub mod error;
mod extract;
pub mod models;
mod truncate;

use exn::ResultExt;
use time::UtcDateTime;
use tracing::instrument;

use crate::error::{ErrorKind, Result};
pub use crate::extract::{Datalist, Extractor, Stats, is_valid};
use crate::models::Version;
pub use crate::truncate::{ESTIMATED_HEADER_SIZE_BYTES, safe_html_truncate};

/// Easy, top-level entrypoint for the extraction of [`Version`] from raw HTML bytes.
///
/// - Automatically truncates large HTML documents, and
/// - Validates the document as part of the extraction.
///
/// Accepts raw bytes, instead of requiring HTML to be valid UTF-8. Invalid byte
/// sequences are replaced with U+FFFD during parsing. See [`Extractor`] for
/// more details.
#[instrument(skip(html), fields(html_size = html.as_ref().len()))]
pub fn extract(html: impl AsRef<[u8]>) -> Result<Version> {
    let html = html.as_ref();
    Ok(Version {
        hash: blake3::hash(html).to_string(),
        crc32: crc32fast::hash(html),
        length: u64::try_from(html.len()).or_raise(|| ErrorKind::ParseError {
            field: "length",
            value: html.len().to_string(),
        })?,
        extracted_at: UtcDateTime::now(),
        metadata: Extractor::from_long_html(html).metadata()?,
    })
}
