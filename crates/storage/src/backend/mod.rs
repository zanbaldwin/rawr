//! Storage backend trait and implementations.
//!
//! This module defines the `StorageBackend` trait, which provides a unified
//! interface for storage operations across different backends (local filesystem,
//! S3-compatible services, etc.).
//!

mod html;
mod local;
#[cfg(feature = "mock")]
mod mock;
mod ro;
#[cfg(feature = "s3")]
mod s3;

pub use self::html::HtmlOnlyBackend;
pub use self::local::LocalBackend;
#[cfg(feature = "mock")]
pub use self::mock::MockBackend;
pub use self::ro::ReadOnlyBackend;
#[cfg(feature = "s3")]
pub use self::s3::S3Backend;
use crate::error::Result;
use crate::file::FileInfo;
use async_trait::async_trait;
use futures::{Stream, TryStreamExt};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;

type FileInfoStream<'a> = Pin<Box<dyn Stream<Item = Result<FileInfo>> + Send + 'a>>;
type BoxSyncRead = Box<dyn Read + Send + 'static>;
type BoxSyncWrite = Box<dyn Write + Send + 'static>;

enum WalkEntry {
    File(FileInfo),
    Descend(PathBuf),
    Skip,
}

/// Unified interface for storage backends.
///
/// All storage operations are asynchronous to efficiently handle network
/// operations and concurrent access. The trait supports both local filesystem
/// and remote storage backends. It's a glorified CRUD interface, but in ✨Rust✨
///
/// # Path Handling
/// All paths are relative to the storage root and must be validated using
/// [`validate_path`](crate::validate_path) before use. Implementations should
/// enforce this validation.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use rawr_storage::{file::FileInfo, backend::StorageBackend, error::Result};
///
/// async fn size_of_hardcoded_file(backend: &dyn StorageBackend) -> Result<u64> {
///     let path = PathBuf::from("path/to/file.html.bz2");
///     if backend.exists(&path).await? {
///         let data = backend.read(&path).await?;
///         Ok(data.len() as u64)
///     } else {
///         Ok(0)
///     }
/// }
/// ```
///
/// # Streaming
/// Read a compressed file, validate its decompressed contents, then
/// re-compress into a different format — all without buffering the
/// entire file in memory:
///
/// ```
/// use std::path::Path;
/// use rawr_compress::Compression;
/// use rawr_storage::backend::StorageBackend;
/// use rawr_storage::error::{ErrorKind, Result};
///
/// async fn transcode_validated(
///     backend: &dyn StorageBackend,
///     source: &Path,
///     target: &Path,
/// ) -> Result<u64> {
///     let source_reader = backend.reader(source).await?;
///     let source_format = Compression::from_path(source);
///
///     let target_writer = backend.writer(target).await?;
///     let target_format = Compression::from_path(target);
///
///     let result_of_thread = tokio::task::spawn_blocking(move || -> Result<u64> {
///         // Decompress
///         let mut peekable = source_format
///             .peekable_reader(source_reader)
///             .map_err(ErrorKind::compression)?;
///         // Validate
///         let head = peekable.peek(15).map_err(ErrorKind::compression)?;
///         assert!(head.starts_with(b"<!DOCTYPE html>"), "not valid HTML5");
///         // Recompress
///         let mut compressor = target_format
///             .wrap_writer(target_writer)
///             .map_err(ErrorKind::compression)?;
///         peekable.copy_into(&mut compressor).map_err(ErrorKind::compression)
///     }).await;
///     result_of_thread.unwrap()
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
    ///
    /// # Notes
    /// - the `prefix` argument may have varying behaviour depending
    ///   on the storage backend implementation used.
    /// - [`list()`](Self::list) is a convenience wrapper that collects this
    ///   stream into a [`Vec`] via [`TryStreamExt`](futures::TryStreamExt::try_collect)
    ///   before returning all at once.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures::TryStreamExt;
    /// use std::path::Path;
    /// # use rawr_storage::{backend::StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    ///
    /// // Filter by prefix
    /// let mut fandom = backend.list_stream(Some(Path::new("Fandom/")));
    ///
    /// // Process files one at a time
    /// let mut stream = backend.list_stream(None);
    /// while let Some(info) = stream.try_next().await? {
    ///     println!("{}: {} bytes", info.path.display(), info.size);
    /// }
    ///
    /// // Process each file as it arrives (up to 4 concurrently)
    /// backend.list_stream(None)
    ///     .try_for_each_concurrent(4, |info| async move {
    ///         println!("{}: {} bytes", info.path.display(), info.size);
    ///         Ok(())
    ///     })
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    fn list_stream<'a>(&'a self, prefix: Option<&'a Path>) -> FileInfoStream<'a>;

    /// Check if a file exists.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{backend::StorageBackend, error::Result};
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
    /// Returns the complete file contents as a [`Vec<u8>`].
    /// Returns [`NotFound`](crate::error::ErrorKind::NotFound) if the file
    /// does not exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{backend::StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// let data = backend.read(Path::new("work.html.bz2")).await?;
    /// println!("Read {} bytes", data.len());
    /// # Ok(())
    /// # }
    /// ```
    async fn read(&self, path: &Path) -> Result<Vec<u8>>;

    /// Read only the first N bytes (for magic byte detection).
    ///
    /// This is useful for detecting file formats without reading the entire
    /// file. Returns [`NotFound`](crate::error::ErrorKind::NotFound) if the
    /// file does not exist.
    ///
    /// # Notes
    /// - This should **NOT** be used for decompression as truncated
    ///   compressed data will fail or return corrupt data.
    /// - If the file is smaller than `bytes`, returns the entire file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use rawr_compress::Compression;
    /// # use rawr_storage::{backend::StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    ///
    /// // Read first 6 bytes to detect compression format
    /// let header = backend.read_head(Path::new("work.html.bz2"), 6).await?;
    /// let format = Compression::from_magic_bytes(&header).unwrap_or(Compression::None);
    /// # Ok(())
    /// # }
    /// ```
    async fn read_head(&self, path: &Path, bytes: usize) -> Result<Vec<u8>>;

    /// Open a file for streaming reads.
    ///
    /// Returns a `'static` boxed [`Read`](std::io::Read) suitable for use
    /// inside [`spawn_blocking`](tokio::task::spawn_blocking). The async
    /// setup (opening the file/connection) happens before returning.
    /// Returns [`NotFound`](crate::error::ErrorKind::NotFound) if the file
    /// does not exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use rawr_compress::Compression;
    /// # use rawr_storage::{backend::StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    ///
    /// let path = Path::new("work.html.bz2");
    /// let file_reader = backend.reader(path).await?;
    /// let format = Compression::from_path(path);
    ///
    /// // Wrap with decompression and read in a blocking task
    /// let html: Vec<u8> = tokio::task::spawn_blocking(move || {
    ///     let mut buf = Vec::new();
    ///     // The decompressor sync Reader can't be sent between threads.
    ///     let mut decompressor = format.wrap_reader(file_reader).unwrap();
    ///     std::io::Read::read_to_end(&mut decompressor, &mut buf).unwrap();
    ///     buf
    /// }).await.unwrap();
    /// # Ok(())
    /// # }
    /// ```
    async fn reader(&self, path: &Path) -> Result<BoxSyncRead>;

    /// Write file contents.
    ///
    /// Creates a new file or overwrites an existing file with the provided data.
    ///
    /// # Notes
    /// - Implementations should create parent directories as needed.
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{backend::StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// let data = b"<html>...</html>";
    /// backend.write(Path::new("work.html"), data).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn write(&self, path: &Path, data: &[u8]) -> Result<()>;

    /// Open a file for streaming writes.
    ///
    /// Returns a `'static` boxed [`Write`](std::io::Write) suitable for use
    /// inside [`spawn_blocking`](tokio::task::spawn_blocking). The async
    /// setup happens before returning.
    ///
    /// # Notes
    /// - Implementations should create parent directories as needed.
    /// - Callers should call `flush()` before dropping to ensure data is
    ///   written and errors are propagated. Some backends (e.g., S3) buffer
    ///   all data and only upload on `flush()`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use rawr_compress::Compression;
    /// # use rawr_storage::{backend::StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    ///
    /// let path = Path::new("work.html.bz2");
    /// let raw_writer = backend.writer(path).await?;
    /// let html = b"<html><body>Hello</body></html>";
    /// let target_format = Compression::from_path(path);
    ///
    /// // Wrap with compression and write in a blocking task
    /// tokio::task::spawn_blocking(move || {
    ///     let mut compressor = target_format.wrap_writer(raw_writer).unwrap();
    ///     std::io::Write::write_all(&mut compressor, html).unwrap();
    /// }).await.unwrap();
    /// # Ok(())
    /// # }
    /// ```
    async fn writer(&self, path: &Path) -> Result<BoxSyncWrite>;

    /// Delete a file.
    ///
    /// Returns [`NotFound`](crate::error::ErrorKind::NotFound) if the file
    /// does not exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{backend::StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// backend.delete(Path::new("old-work.html.bz2")).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn delete(&self, path: &Path) -> Result<()>;

    /// Rename/move a file within the same backend.
    ///
    /// Returns [`NotFound`](crate::error::ErrorKind::NotFound) if the source
    /// file does not exist.
    ///
    /// # Notes
    /// - Implementations should create parent directories as needed
    /// - If the destination already exists, it will be overwritten
    /// - For non-atomic backends: warn but don't fail when the delete operation fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{backend::StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// backend.rename(
    ///     Path::new("old-path.html.bz2"),
    ///     Path::new("new-path.html.bz2")
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn rename(&self, from: &Path, to: &Path) -> Result<()>;

    /// Get file metadata without reading contents.
    ///
    /// Returns [`NotFound`](crate::error::ErrorKind::NotFound) if the file
    /// does not exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// # use rawr_storage::{backend::StorageBackend, error::Result};
    /// # async fn example(backend: &dyn StorageBackend) -> Result<()> {
    /// let info = backend.stat(Path::new("work.html.bz2")).await?;
    /// println!("Size: {} bytes, Discovered: {}", info.size, info.discovered_at);
    /// # Ok(())
    /// # }
    /// ```
    async fn stat(&self, path: &Path) -> Result<FileInfo>;
}
