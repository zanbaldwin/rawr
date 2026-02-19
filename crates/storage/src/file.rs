//! File metadata types for storage (and cache) operations. [`FileInfo`] tracks
//! whether a file's content hash has been computed using a typestate parameter:
//!
//! - [`FileInfo<Discovered>`] (or just [`FileInfo`]) — metadata only, no hash yet
//! - [`FileInfo<Read>`] — file hash computed and available as a [`String`]
//! - [`FileInfo<Processed>`] - content hash computed and available as a [`String`]
//!
//! Both dereference to [`FileMeta`], which holds the common fields.
//!
//! # Lifecycle
//!
//! ```no_run
//! # use rawr_storage::file::{FileInfo, Read};
//! # use rawr_compress::Compression;
//! # use time::UtcDateTime;
//! // Backends return FileInfo<Discovered> (the default)
//! let file = FileInfo::new(
//!     "local",
//!     "work/123.html.gz",
//!     4096,
//!     UtcDateTime::now(),
//!     Compression::Gzip,
//! );
//! // Access metadata fields directly via Deref
//! println!("{}: {} bytes", file.path.display(), file.size);
//!
//! // Attach a hash to transition to Read state
//! let file = file.with_file_hash("af1349b9f5f9a1a6...");
//! // file_hash is now a String, not unit ()
//! println!("{}: {}", file.path.display(), file.file_hash);
//! ```
//!
//! # Choosing a Type for Function Signatures
//!
//! | Accepts                            | Use when                                                  |
//! |------------------------------------|-----------------------------------------------------------|
//! | [`&FileMeta`](FileMeta)            | Only metadata needed — works with any state via [`Deref`] |
//! | [`&FileInfo`](FileInfo)            | Working with unhashed files from backend listings         |
//! | [`&FileInfo<Read>`](FileInfo)      | File hash is required at compile time                     |
//! | [`&FileInfo<Processed>`](FileInfo) | Content hash is required at compile time                  |

use rawr_compress::Compression;
use std::{ops::Deref, path::PathBuf};
use time::UtcDateTime;

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
    pub target: String,
    /// Relative path from the storage root
    pub path: PathBuf,
    /// Compression format (detected from the file extension)
    pub compression: Compression,
    pub size: u64,
    pub discovered_at: UtcDateTime,
}
impl FileMeta {
    pub fn new(
        target: impl Into<String>,
        path: impl Into<PathBuf>,
        compression: Compression,
        size: u64,
        discovered_at: UtcDateTime,
    ) -> Self {
        Self {
            target: target.into(),
            path: path.into(),
            compression,
            size,
            discovered_at,
        }
    }

    /// Consumes itself to attach a hash, transitioning to [`FileInfo<Read>`].
    pub fn with_file_hash(self, hash: impl Into<String>) -> FileInfo<Read> {
        FileInfo {
            meta: self,
            file_hash: hash.into(),
            content_hash: (),
        }
    }
}

mod sealed {
    pub trait Sealed {}
}
/// Represents the hash calculation state of a [`FileInfo`].
///
/// This trait is sealed and cannot be implemented outside this crate.
/// The three implementations are [`Discovered`], [`Read`] and [`Processed`].
pub trait HashState: sealed::Sealed {
    /// - `()` for [`Discovered`]
    /// - [`String`] for [`Read`] and [`Processed`]
    type File;
    /// - `()` for [`Discovered`] and [`Read`]
    /// - [`String`] for [`Processed`]
    type Content;
}

/// Hash state: file discovered, hashes not yet computed.
///
/// This is the default state for [`FileInfo`] returned from
/// [`StorageBackend`](crate::StorageBackend) operations.
pub struct Discovered;
impl sealed::Sealed for Discovered {}
impl HashState for Discovered {
    type File = ();
    type Content = ();
}

/// Hash state: file hash has been computed.
///
/// [`FileInfo<Read>`] provides the file hash as a [`String`] via
/// the [`file_hash`](FileInfo::file_hash) field.
pub struct Read;
impl sealed::Sealed for Read {}
impl HashState for Read {
    type File = String;
    type Content = ();
}

/// Hash state: content hash has been computed.
///
/// [`FileInfo<Processed>`] provides the content hash as a [`String`] via
/// the [`content_hash`](FileInfo::content_hash) field.
#[derive(Debug, Eq, PartialEq)]
pub struct Processed;
impl sealed::Sealed for Processed {}
impl HashState for Processed {
    type File = String;
    type Content = String;
}

/// File metadata with typestate-tracked hash status.
///
/// Wraps [`FileMeta`] and adds a hash field whose type depends on the
/// state parameter `S`. Dereferences to [`FileMeta`] for direct field
/// access.
///
/// See the [module documentation](self) for usage examples and guidance
/// on choosing between `FileMeta`, `FileInfo`, `FileInfo<Read>`, and
/// `FileInfo<Processed>` in function signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo<S: HashState = Discovered> {
    meta: FileMeta,
    /// - `()` in [`Discovered`] state
    /// - [`String`] in [`Read`] and [`Processed`] states
    pub file_hash: S::File,
    /// - `()` in [`Discovered`] and [`Read`] states
    /// - [`String`] in [`Processed`] state
    pub content_hash: S::Content,
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

    pub fn strip_hashes(self) -> FileInfo<Discovered> {
        self.meta.into()
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
    pub fn new(
        target: impl Into<String>,
        path: impl Into<PathBuf>,
        size: u64,
        discovered_at: UtcDateTime,
        compression: Compression,
    ) -> Self {
        FileMeta::new(target, path, compression, size, discovered_at).into()
    }

    /// Consumes itself to attach a hash, transitioning to [`FileInfo<Read>`].
    pub fn with_file_hash(self, hash: impl Into<String>) -> FileInfo<Read> {
        FileInfo {
            meta: self.meta,
            file_hash: hash.into(),
            content_hash: (),
        }
    }
}
impl From<FileMeta> for FileInfo<Discovered> {
    fn from(meta: FileMeta) -> Self {
        Self { meta, file_hash: (), content_hash: () }
    }
}

impl FileInfo<Read> {
    /// Consumes itself to attach a hash, transitioning to [`FileInfo<Processed>`].
    pub fn with_content_hash(self, hash: impl Into<String>) -> FileInfo<Processed> {
        FileInfo {
            meta: self.meta,
            file_hash: self.file_hash,
            content_hash: hash.into(),
        }
    }
}
