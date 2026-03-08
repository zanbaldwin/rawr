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
mod opendal_util;
mod ro;
#[cfg(feature = "s3")]
mod s3;

pub use self::html::HtmlOnlyBackend;
pub use self::local::LocalBackend;
#[cfg(feature = "mock")]
pub use self::mock::MockBackend;
use self::opendal_util::{map_opendal_error, metadata_to_file_info};
pub use self::ro::ReadOnlyBackend;
#[cfg(feature = "s3")]
pub use self::s3::S3Backend;
use crate::error::{ErrorKind, Result};
use crate::file::FileInfo;
use crate::path::ValidPath;
use async_stream::stream;
use async_trait::async_trait;
use futures::io::{AsyncRead, AsyncWrite};
use futures::{Stream, StreamExt, TryStreamExt};
use opendal::Operator;
use std::path::Path;
use std::pin::Pin;

type FileInfoStream<'a> = Pin<Box<dyn Stream<Item = Result<FileInfo>> + Send + 'a>>;

/// Boxed async reader returned by [`StorageBackend::reader()`].
pub type BoxedReader = Box<dyn AsyncRead + Unpin + Send + 'static>;
/// Boxed async writer returned by [`StorageBackend::writer()`].
pub type BoxedWriter = Box<dyn AsyncWrite + Unpin + Send + 'static>;

/// Private Access to the underlying OpenDAL operator
pub(crate) trait OperatorAware {
    fn operator(&self) -> &Operator;
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
#[async_trait]
#[allow(private_bounds)]
pub trait StorageBackend: OperatorAware + Send + Sync {
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
    async fn list(&self, prefix: Option<&ValidPath>) -> Result<Vec<FileInfo>> {
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
    /// let mut fandom = backend
    ///     .list_stream(Some(Path::new("Fandom/")))
    ///     .expect("the prefix should be valid");
    ///
    /// // Process files one at a time
    /// let mut stream = backend.list_stream(None).expect("the prefix should be valid");
    /// while let Some(info) = stream.try_next().await? {
    ///     println!("{}: {} bytes", info.path.display(), info.size);
    /// }
    ///
    /// // Process each file as it arrives (up to 4 concurrently)
    /// backend.list_stream(None)
    ///     .expect("the prefix should be valid")
    ///     .try_for_each_concurrent(4, |info| async move {
    ///         println!("{}: {} bytes", info.path.display(), info.size);
    ///         Ok(())
    ///     })
    ///     .await?;
    ///
    /// // Prefixes shouldn't attempt to break storage
    /// assert!(backend.list_stream(Some("../../etc/passwd")).is_err());
    /// # Ok(())
    /// # }
    /// ```
    // TODO: Change Path to Str?
    fn list_stream<'a>(&'a self, prefix: Option<&'a ValidPath>) -> FileInfoStream<'a> {
        tracing::trace!(
            backend = self.name(),
            prefix = %prefix.map(|p|p.as_str()).unwrap_or_default(),
            "stream list of files from storage backend"
        );
        let opendal_prefix = prefix.map(|p| format!("{}/", p.trim_end_matches('/'))).unwrap_or_else(|| "/".to_string());

        Box::pin(stream! {
            let mut lister = match self.operator().lister_with(&opendal_prefix).recursive(true).await {
                Ok(l) => l,
                Err(e) if matches!(e.kind(), opendal::ErrorKind::NotFound) => return,
                Err(e) => {
                    yield Err(exn::Exn::from(map_opendal_error(e, Path::new(&opendal_prefix))));
                    return;
                },
            };
            while let Some(entry_result) = lister.next().await {
                match entry_result {
                    Ok(entry) => {
                        let path_str = entry.path();
                        if path_str.ends_with('/') { continue; }
                        let relative = match ValidPath::new(path_str) {
                            Ok(p) => p,
                            Err(e) => { yield Err(e); continue; }
                        };
                        if let Some(pfx) = &prefix && !relative.as_str().starts_with(pfx.as_str()) { continue; }
                        yield Ok(metadata_to_file_info(self.name(), relative.into(), entry.metadata()));
                    },
                    Err(e) if !matches!(e.kind(), opendal::ErrorKind::NotFound) => {
                        yield Err(exn::Exn::from(map_opendal_error(e, Path::new(&opendal_prefix))));
                    },
                    Err(_) => continue,
                }
            }
        })
    }

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
    async fn exists(&self, path: &ValidPath) -> Result<bool> {
        tracing::trace!(backend = self.name(), path = %path, "check file existence in storage backend");
        self.operator().exists(path.as_str()).await.map_err(|e| map_opendal_error(e, path.as_path()).into())
    }

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
    async fn read(&self, path: &ValidPath) -> Result<Vec<u8>> {
        tracing::trace!(backend = self.name(), path = %path, "read file from storage backend");
        let data = self.operator().read(path.as_str()).await.map_err(|e| map_opendal_error(e, path.as_path()))?;
        Ok(data.to_vec())
    }

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
    async fn read_head(&self, path: &ValidPath, bytes: usize) -> Result<Vec<u8>> {
        tracing::trace!(backend = self.name(), path = %path, bytes, "read initial bytes range of file from storage backend");
        let meta = self.operator().stat(path.as_str()).await.map_err(|e| map_opendal_error(e, path.as_path()))?;
        let actual_len = meta.content_length();
        let end = (bytes as u64).min(actual_len);
        let data = self
            .operator()
            .read_with(path.as_str())
            .range(..end)
            .await
            .map_err(|e| map_opendal_error(e, path.as_path()))?;
        Ok(data.to_vec())
    }

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
    async fn write(&self, path: &ValidPath, data: &[u8]) -> Result<()> {
        tracing::trace!(backend = self.name(), path = %path, bytes = data.len(), "write file to storage backend");
        self.operator().write(path.as_str(), data.to_vec()).await.map_err(|e| map_opendal_error(e, path.as_path()))?;
        Ok(())
    }

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
    async fn delete(&self, path: &ValidPath) -> Result<()> {
        tracing::trace!(backend = self.name(), path = %path, "delete file from storage backend");
        if !self.exists(path).await? {
            exn::bail!(ErrorKind::NotFound(path.to_string()));
        }
        self.operator().delete(path.as_str()).await.map_err(|e| map_opendal_error(e, path.as_path()))?;
        Ok(())
    }

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
    async fn rename(&self, from: &ValidPath, to: &ValidPath) -> Result<()> {
        tracing::trace!(backend = self.name(), from = %from, to = %to, "rename file in storage backend");
        self.operator().rename(from.as_str(), to.as_str()).await.map_err(|e| map_opendal_error(e, from.as_path()))?;
        Ok(())
    }

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
    async fn stat(&self, path: &ValidPath) -> Result<FileInfo> {
        tracing::trace!(backend = self.name(), path = %path, "get file metadata from storage backend");
        let meta = self.operator().stat(path.as_str()).await.map_err(|e| map_opendal_error(e, path.as_path()))?;
        Ok(metadata_to_file_info(self.name(), path.as_path().to_path_buf(), &meta))
    }

    /// Open a file for streaming reads.
    ///
    /// Returns an async reader that streams file contents incrementally.
    /// Returns [`NotFound`](crate::error::ErrorKind::NotFound) if the file
    /// does not exist.
    async fn reader(&self, path: &ValidPath) -> Result<BoxedReader> {
        tracing::trace!(backend = self.name(), path = %path, "open reader to file in storage backend");
        let reader = self.operator().reader(path.as_str()).await.map_err(|e| map_opendal_error(e, path.as_path()))?;
        let async_read = reader.into_futures_async_read(..).await.map_err(|e| map_opendal_error(e, path.as_path()))?;
        Ok(Box::new(async_read))
    }

    /// Open a file for streaming writes.
    ///
    /// Returns an async writer that streams data to storage. The caller
    /// **must** call [`AsyncWriteExt::close()`](futures::io::AsyncWriteExt::close)
    /// on the returned writer to finalize the write operation.
    ///
    /// Creates parent directories as needed (consistent with `write()`).
    async fn writer(&self, path: &ValidPath) -> Result<BoxedWriter> {
        tracing::trace!(backend = self.name(), path = %path, "open writer to file in storage backend");
        let writer = self.operator().writer(path.as_str()).await.map_err(|e| map_opendal_error(e, path.as_path()))?;
        Ok(Box::new(writer.into_futures_async_write()))
    }
}
