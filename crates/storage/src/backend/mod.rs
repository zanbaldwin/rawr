//! Storage backend trait and implementations.
//!
//! This module defines the `StorageBackend` trait, which provides a unified
//! interface for storage operations across different backends (local filesystem,
//! S3-compatible services, etc.).
//!

mod html;
mod local;
mod ro;
#[cfg(feature = "s3")]
mod s3;

pub use self::html::HtmlOnlyBackend;
pub use self::local::LocalBackend;
pub use self::ro::ReadOnlyBackend;
#[cfg(feature = "s3")]
pub use self::s3::S3Backend;
use crate::{error::Result, file::FileInfo};
use async_trait::async_trait;
use futures::{Stream, TryStreamExt};
use std::path::{Path, PathBuf};
use std::pin::Pin;

type FileInfoStream<'a> = Pin<Box<dyn Stream<Item = Result<FileInfo>> + Send + 'a>>;

enum WalkEntry {
    File(FileInfo),
    Descend(PathBuf),
    Skip,
}

/// Unified interface for storage backends.
///
/// All storage operations are asynchronous to efficiently handle network
/// operations and concurrent access. The trait supports both local filesystem
/// and remote storage backends.
///
/// # Path Handling
/// All paths are relative to the storage root and must be validated using
/// [`validate_path`](crate::validate_path) before use. Implementations should
/// enforce this validation.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use rawr_storage::{FileInfo, StorageBackend, error::Result};
/// async fn example(backend: &dyn StorageBackend) -> Result<u64> {
///     if backend.exists(Path::new("path/to/file.html.bz2")).await? {
///         let data = backend.read(Path::new("path/to/file.html.bz2")).await?;
///         Ok(data.len() as u64)
///     } else {
///         Ok(0)
///     }
/// }
/// ```
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Name of the configured backend (name taken from the configuration
    /// object key). Each backend's name is **supposed** to be unique, but it
    /// doesn't affect the functionality of this crate if they aren't (used
    /// for logging only).
    fn name(&self) -> &str;

    /// List all files matching an optional prefix.
    ///
    /// Default implementation of this method is to collect all the results
    /// from [`list_stream()`](Self::list_stream) into a [`Vec`] before
    /// returning.
    async fn list(&self, prefix: Option<&Path>) -> Result<Vec<FileInfo>> {
        self.list_stream(prefix).try_collect().await
    }

    /// Stream file metadata matching an optional prefix.
    ///
    /// Returns metadata for all files in the storage backend as a
    /// [`Stream`], yielding results incrementally and immediately. If a
    /// prefix is provided, only files whose paths start with the prefix
    /// are returned.
    ///
    /// [`list()`](Self::list) is a convenience wrapper that collects this
    /// stream via [`TryStreamExt`](futures::TryStreamExt::try_collect).
    ///
    /// # Arguments
    ///
    /// - `prefix` - Optional path prefix to filter results
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use futures::TryStreamExt;
    /// use std::path::Path;
    /// # use rawr_storage::{StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// // Process files one at a time
    /// let mut stream = backend.list_stream(None);
    /// while let Some(info) = stream.try_next().await? {
    ///     println!("{}: {} bytes", info.path.display(), info.size);
    /// }
    ///
    /// // Filter by prefix
    /// let mut fandom = backend.list_stream(Some(Path::new("Fandom/")));
    /// # Ok(())
    /// # }
    /// ```
    fn list_stream<'a>(&'a self, prefix: Option<&'a Path>) -> FileInfoStream<'a>;

    /// Check if a file exists.
    ///
    /// # Arguments
    ///
    /// * `path` - Relative path from storage root
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// if backend.exists(Path::new("work.html.bz2")).await? {
    ///     println!("File exists!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn exists(&self, path: &Path) -> Result<bool>;

    /// Read file contents.
    ///
    /// Returns the complete file contents as a byte vector.
    ///
    /// # Arguments
    ///
    /// * `path` - Relative path from storage root
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] if the file does not exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// let data = backend.read(Path::new("work.html.bz2")).await?;
    /// println!("Read {} bytes", data.len());
    /// # Ok(())
    /// # }
    /// ```
    async fn read(&self, path: &Path) -> Result<Vec<u8>>;

    /// Read only the first N bytes (for magic byte detection).
    ///
    /// This is useful for detecting file formats without reading the entire file.
    ///
    /// > **Note:** This should NOT be used for extraction, as decompression of
    /// > truncated compressed data will fail or return corrupt data.
    ///
    /// # Arguments
    /// * `path` - Relative path from storage root
    /// * `bytes` - Maximum number of bytes to read
    ///
    /// # Returns
    /// Returns up to `bytes` bytes from the start of the file. If the file is
    /// smaller than `bytes`, returns the entire file.
    ///
    /// # Errors
    /// Returns [`Error::NotFound`] if the file does not exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{StorageBackend, error::Result};
    /// # use rawr_compress::Compression;
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// // Read first 6 bytes to detect compression format
    /// let header = backend.read_head(Path::new("work.html.bz2"), 6).await?;
    /// let format = Compression::from_magic_bytes(&header);
    /// # Ok(())
    /// # }
    /// ```
    async fn read_head(&self, path: &Path, bytes: usize) -> Result<Vec<u8>>;

    /// Write file contents.
    ///
    /// Creates a new file or overwrites an existing file with the provided data.
    ///
    /// # Arguments
    /// * `path` - Relative path from storage root
    /// * `data` - File contents to write
    ///
    /// # Notes
    /// Implementations should create parent directories as needed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// let data = b"<html>...</html>";
    /// backend.write(Path::new("work.html"), data).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn write(&self, path: &Path, data: &[u8]) -> Result<()>;

    /// Delete a file.
    ///
    /// # Arguments
    /// * `path` - Relative path from storage root
    ///
    /// # Errors
    /// Returns [`Error::NotFound`] if the file does not exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// backend.delete(Path::new("old-work.html.bz2")).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn delete(&self, path: &Path) -> Result<()>;

    /// Rename/move a file within the same backend.
    ///
    /// # Arguments
    /// * `from` - Current path (relative to storage root)
    /// * `to` - New path (relative to storage root)
    ///
    /// # Notes
    /// - Implementations should create parent directories as needed
    /// - For S3 backends, this is implemented as copy + delete
    /// - If the destination already exists, it will be overwritten
    ///
    /// # Errors
    /// Returns [`Error::NotFound`] if the source file does not exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// backend.rename(Path::new("old-path.html.bz2"), Path::new("new-path.html.bz2")).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn rename(&self, from: &Path, to: &Path) -> Result<()>;

    /// Get file metadata without reading contents.
    ///
    /// # Arguments
    /// * `path` - Relative path from storage root
    ///
    /// # Errors
    /// Returns [`NotFound`](crate::error::ErrorKind::NotFound) if the file
    /// does not exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// let info = backend.stat(Path::new("work.html.bz2")).await?;
    /// println!("Size: {} bytes, Modified: {}", info.size, info.modified);
    /// # Ok(())
    /// # }
    /// ```
    async fn stat(&self, path: &Path) -> Result<FileInfo>;
}
