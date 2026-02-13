//! Storage models.
//!
//! These types represent both database rows (for the cache) and storage
//! backend metadata (for listing and sync operations).

use rawr_compress::Compression;
use std::path::PathBuf;
use time::OffsetDateTime;

/// File metadata returned by storage backends.
///
/// This represents information about a file in storage, used for listing
/// operations and sync comparisons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    /// Relative path from storage root
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Last modified timestamp
    pub modified: OffsetDateTime,
    /// Detected compression format from file extension
    pub compression: Compression,
    /// BLAKE3 hash of file content (populated after reading file during scan)
    pub file_hash: Option<String>,
}
impl FileInfo {
    /// Create a new FileInfo from a listing operation (no hash yet).
    pub fn new(path: impl Into<PathBuf>, size: u64, modified: OffsetDateTime, compression: Compression) -> Self {
        Self {
            path: path.into(),
            size,
            modified,
            compression,
            file_hash: None,
        }
    }

    pub fn with_file_hash(mut self, file_hash: impl Into<String>) -> Self {
        self.file_hash = Some(file_hash.into());
        self
    }
}
