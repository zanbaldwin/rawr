use super::Metadata;
use time::{Date, UtcDateTime};

/// A specific version of an AO3 work, representing the metadata extracted from
/// a single HTML download. This is the primary entity in the system.
///
/// The only fields on an AO3 work that never change are `work_id` and `published`.
/// All other metadata (title, authors, chapters, etc.) can theoretically change
/// between downloads, so each unique download is tracked as a separate Version.
///
/// Multiple Versions can exist for the same `work_id`. The `content_hash` serves
/// as the primary key, providing natural deduplication: if two files have identical
/// decompressed content, they reference the same Version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// BLAKE3 hash of decompressed HTML (primary key)
    pub hash: String,
    /// Uncompressed HTML length in bytes (for quick/cheap equality check)
    pub length: u64,
    /// CRC32 hash of decompressed HTML
    pub crc32: u32,
    pub metadata: Metadata,
    pub extracted_at: UtcDateTime,
}
impl AsRef<Version> for Version {
    fn as_ref(&self) -> &Version {
        self
    }
}
impl Version {
    /// Returns the most recent modification date for this version.
    pub fn last_modified(&self) -> Date {
        self.metadata.last_modified
    }
}
