//! These types represent both database rows (for the cache) and storage
//! backend metadata (for listing and sync operations).

use rawr_compress::Compression;
use std::{ops::Deref, path::PathBuf};
use time::OffsetDateTime;

// Note to self: I've never used the typestate pattern and I _really_ want to
// use it here. Come back here in the future when it comes back to bite you
// in the ass so that you can tell yourself "I told you so". In case you need
// reminding: it will infect your call stack and you you'll have to deref to
// FileMeta in order to have mixed collections.

/// File metadata returned by storage backends.
///
/// This represents information about a file in storage, used for listing
/// operations and sync comparisons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    /// Relative path from storage root
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Last modified timestamp
    pub modified: OffsetDateTime,
    /// Detected compression format from file extension
    pub compression: Compression,
}
impl FileMeta {
    pub fn with_hash(self, hash: impl Into<String>) -> FileInfo<Calculated> {
        FileInfo { meta: self, file_hash: hash.into() }
    }
}

mod sealed {
    pub trait Sealed {}
}
pub trait HashState: sealed::Sealed {
    type Hash;
}

pub struct Discovered;
impl sealed::Sealed for Discovered {}
impl HashState for Discovered {
    type Hash = ();
}

pub struct Calculated;
impl sealed::Sealed for Calculated {}
impl HashState for Calculated {
    type Hash = String;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo<S: HashState = Discovered> {
    meta: FileMeta,
    pub file_hash: S::Hash,
}
impl<S: HashState> FileInfo<S> {
    // Explicit version of self.as_deref()
    pub fn meta(&self) -> &FileMeta {
        &self.meta
    }

    pub fn into_meta(self) -> FileMeta {
        self.meta
    }
}
impl<S: HashState> Deref for FileInfo<S> {
    type Target = FileMeta;
    fn deref(&self) -> &FileMeta {
        &self.meta
    }
}

impl FileInfo {
    /// Create a new FileInfo from a listing operation (no hash yet).
    pub fn new(path: impl Into<PathBuf>, size: u64, modified: OffsetDateTime, compression: Compression) -> Self {
        FileMeta {
            path: path.into(),
            size,
            modified,
            compression,
        }
        .into()
    }

    pub fn with_hash(self, hash: impl Into<String>) -> FileInfo<Calculated> {
        FileInfo { meta: self.meta, file_hash: hash.into() }
    }
}
impl From<FileMeta> for FileInfo<Discovered> {
    fn from(meta: FileMeta) -> Self {
        Self { meta, file_hash: () }
    }
}
