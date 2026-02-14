//! File metadata types for storage (and cache) operations. [`FileInfo`] tracks
//! whether a file's content hash has been computed using a typestate parameter:
//!
//! - [`FileInfo<Discovered>`] (or just [`FileInfo`]) — metadata only, no hash yet
//! - [`FileInfo<Calculated>`] — hash computed and available as a [`String`]
//!
//! Both dereference to [`FileMeta`], which holds the common fields.
//!
//! # Lifecycle
//!
//! ```no_run
//! # use rawr_storage::file::{FileInfo, Calculated};
//! # use rawr_compress::Compression;
//! # use time::OffsetDateTime;
//! // Backends return FileInfo<Discovered> (the default)
//! let file = FileInfo::new(
//!     "work/123.html.gz",
//!     4096,
//!     OffsetDateTime::now_utc(),
//!     Compression::Gzip,
//! );
//! // Access metadata fields directly via Deref
//! println!("{}: {} bytes", file.path.display(), file.size);
//!
//! // Attach a hash to transition to Calculated
//! let file = file.with_hash("af1349b9f5f9a1a6...");
//! // file_hash is now a String, not unit ()
//! println!("{}: {}", file.path.display(), file.file_hash);
//! ```
//!
//! # Choosing a Type for Function Signatures
//!
//! | Accepts | Use when |
//! |---|---|
//! | [`&FileMeta`](FileMeta) | Only metadata needed — works with any state via [`Deref`] |
//! | [`&FileInfo`](FileInfo) | Working with unhashed files from backend listings |
//! | [`&FileInfo<Calculated>`](FileInfo) | Hash is required at compile time |

use rawr_compress::Compression;
use std::{ops::Deref, path::PathBuf};
use time::OffsetDateTime;

// Note to self: I've never used the typestate pattern and I _really_ want to
// use it here. Come back here in the future when it comes back to bite you
// in the ass so that you can tell yourself "I told you so". In case you need
// reminding: it will infect your call stack and you you'll have to deref to
// FileMeta in order to have mixed collections.

/// Core file metadata from a storage backend.
///
/// [`FileInfo`] dereferences to this type, so these fields are accessible
/// on any [`FileInfo<S>`](FileInfo) directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    /// Relative path from the storage root
    pub path: PathBuf,
    pub size: u64,
    pub modified: OffsetDateTime,
    /// Compression format (detected from the file extension)
    pub compression: Compression,
}
impl FileMeta {
    /// Consumes itself to attach a hash, transitioning to [`FileInfo<Calculated>`].
    pub fn with_hash(self, hash: impl Into<String>) -> FileInfo<Calculated> {
        FileInfo { meta: self, file_hash: hash.into() }
    }
}

mod sealed {
    pub trait Sealed {}
}
/// Represents the hash calculation state of a [`FileInfo`].
///
/// This trait is sealed and cannot be implemented outside this crate.
/// The two implementations are [`Discovered`] and [`Calculated`].
pub trait HashState: sealed::Sealed {
    /// - `()` for [`Discovered`]
    /// - [`String`] for [`Calculated`]
    type Hash;
}

/// Hash state: file discovered, hash not yet computed.
///
/// This is the default state for [`FileInfo`] returned from
/// [`StorageBackend`](crate::StorageBackend) operations.
pub struct Discovered;
impl sealed::Sealed for Discovered {}
impl HashState for Discovered {
    type Hash = ();
}

/// Hash state: content hash has been computed.
///
/// [`FileInfo<Calculated>`] provides the hash as a [`String`] via
/// the [`file_hash`](FileInfo::file_hash) field.
pub struct Calculated;
impl sealed::Sealed for Calculated {}
impl HashState for Calculated {
    type Hash = String;
}

/// File metadata with typestate-tracked hash status.
///
/// Wraps [`FileMeta`] and adds a hash field whose type depends on the
/// state parameter `S`. Dereferences to [`FileMeta`] for direct field
/// access.
///
/// See the [module documentation](self) for usage examples and guidance
/// on choosing between `FileMeta`, `FileInfo`, and `FileInfo<Calculated>`
/// in function signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo<S: HashState = Discovered> {
    meta: FileMeta,
    /// - `()` in [`Discovered`] state
    /// - [`String`] in [`Calculated`] state
    pub file_hash: S::Hash,
}
impl<S: HashState> FileInfo<S> {
    /// Returns a reference to the underlying [`FileMeta`].
    ///
    /// Explicitly named version of [`deref()`](Deref::deref).
    pub fn meta(&self) -> &FileMeta {
        self.deref()
    }

    /// Consumes this file info and returns the underlying [`FileMeta`].
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
    /// Creates a new file info in the [`Discovered`] state.
    pub fn new(path: impl Into<PathBuf>, size: u64, modified: OffsetDateTime, compression: Compression) -> Self {
        FileMeta {
            path: path.into(),
            size,
            modified,
            compression,
        }
        .into()
    }

    /// Consumes itself to attach a hash, transitioning to [`FileInfo<Calculated>`].
    pub fn with_hash(self, hash: impl Into<String>) -> FileInfo<Calculated> {
        FileInfo { meta: self.meta, file_hash: hash.into() }
    }
}
impl From<FileMeta> for FileInfo<Discovered> {
    fn from(meta: FileMeta) -> Self {
        Self { meta, file_hash: () }
    }
}
