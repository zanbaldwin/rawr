//! Scanning and metadata extraction for files in storage backends.
//!
//! This module discovers files from a [storage backend](rawr_storage),
//! [extracts](rawr_extract) their metadata, and [caches](rawr_cache::Repository)
//! results to avoid redundant work. It operates at two levels:
//!
//! - **Single-file**: [`scan_file`] processes one
//!   [`FileInfo`](rawr_storage::file::FileInfo) through a multi-layered cache
//!   lookup (path match, hash match, content dedup) before falling back to
//!   full extraction.
//! - **Streaming**: [`scan`] concurrently scans an entire backend, emitting
//!   [`ScanEvent`]s that separate file discovery from processing — enabling
//!   progress reporting with known totals.

pub(crate) mod error;
pub(crate) mod file;
mod stream;

pub use self::file::{Scan, ScanEffort, scan_file};
pub use self::stream::{ScanEvent, scan};
