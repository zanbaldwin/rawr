//! Local filesystem storage backend.
//!
//! This module provides a storage backend implementation for the local filesystem
//! using [OpenDAL](https://docs.rs/opendal/) with the `Fs` service for async I/O.

use crate::StorageBackend;
use crate::backend::OperatorAware;
use crate::error::{ErrorKind, Result};
use async_trait::async_trait;
use opendal::services::Fs;
use opendal::{Operator, layers::RetryLayer};
use std::fs::create_dir_all as sync_create_dir;
use std::path::Path;

/// Local filesystem storage backend.
///
/// Stores files in a directory on the local filesystem.
/// All paths are relative to the configured root directory.
///
/// # Examples
///
/// ```no_run
/// use rawr_storage::backend::LocalBackend;
/// use std::path::PathBuf;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = LocalBackend::new("local", "/path/to/library")?;
/// # Ok(())
/// # }
/// ```
pub struct LocalBackend {
    name: String,
    operator: Operator,
}
impl LocalBackend {
    /// Create a new local filesystem backend.
    ///
    /// Returns an [`InvalidPath`](crate::error::ErrorKind::InvalidPath) if
    /// the path is not absolute.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use rawr_storage::backend::LocalBackend;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = LocalBackend::new("nfs", "/absolute/path/to/library")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(name: impl Into<String>, root: impl AsRef<Path>, auto_create: bool) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        if !root.is_absolute() {
            exn::bail!(ErrorKind::InvalidPath(root));
        }
        if root.exists() {
            if !root.is_dir() {
                exn::bail!(ErrorKind::InvalidPath(root));
            }
        } else if auto_create {
            // Use non-async here; it'll only happen once on library initialization
            // and it's not worth the hassle of making the constructor async.
            sync_create_dir(&root).map_err(ErrorKind::Io)?;
        } else {
            exn::bail!(ErrorKind::PermissionDenied(root));
        }

        let root_str = root.to_str().ok_or_else(|| ErrorKind::InvalidPath(root.clone()))?;
        let builder = Fs::default().root(root_str);
        let operator = Operator::new(builder)
            .map_err(|e| ErrorKind::BackendError(e.to_string()))?
            .layer(RetryLayer::default())
            .finish();

        Ok(Self { name: name.into(), operator })
    }
}

impl OperatorAware for LocalBackend {
    fn operator(&self) -> &Operator {
        &self.operator
    }
}
#[async_trait]
impl StorageBackend for LocalBackend {
    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use crate::error::ErrorKind;
    use futures::io::{AsyncReadExt, AsyncWriteExt};
    use rawr_compress::Compression;
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_new_requires_absolute_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert!(LocalBackend::new("name", temp_dir.path(), false).is_ok());
        assert!(LocalBackend::new("name", "relative/path", false).is_err());
        assert!(LocalBackend::new("name", "./relative", false).is_err());
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        let data = b"Hello, world!";
        backend.write(Path::new("test.txt"), data).await.unwrap();
        let read_data = backend.read(Path::new("test.txt")).await.unwrap();
        assert_eq!(read_data, data);
    }

    #[tokio::test]
    async fn test_prefix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        let data = b"Hello, world!";
        backend.write(Path::new("FandomA/Sub/file.html"), data).await.unwrap();
        backend.write(Path::new("FandomA/Subdir/file.html"), data).await.unwrap();
        backend.write(Path::new("FandomA/Subfile.html"), data).await.unwrap();
        let mut files = backend.list(Some(Path::new("FandomA/Sub"))).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files.pop().unwrap().path, Path::new("FandomA/Sub/file.html"));
    }

    #[tokio::test]
    async fn test_write_creates_directories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        backend.write(Path::new("a/b/c/file.txt"), b"data").await.unwrap();
        assert!(backend.exists(Path::new("a/b/c/file.txt")).await.unwrap());
    }

    #[tokio::test]
    async fn test_exists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        assert!(!backend.exists(Path::new("nonexistent.txt")).await.unwrap());
        backend.write(Path::new("exists.txt"), b"data").await.unwrap();
        assert!(backend.exists(Path::new("exists.txt")).await.unwrap());
    }

    #[tokio::test]
    async fn test_read_head() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        let data = b"0123456789ABCDEF";
        backend.write(Path::new("file.txt"), data).await.unwrap();
        let head = backend.read_head(Path::new("file.txt"), 5).await.unwrap();
        assert_eq!(head, b"01234");
        // Reading more bytes than file size returns entire file
        let all = backend.read_head(Path::new("file.txt"), 100).await.unwrap();
        assert_eq!(all, data);
    }

    #[tokio::test]
    async fn test_delete() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        backend.write(Path::new("file.txt"), b"data").await.unwrap();
        assert!(backend.exists(Path::new("file.txt")).await.unwrap());
        backend.delete(Path::new("file.txt")).await.unwrap();
        assert!(!backend.exists(Path::new("file.txt")).await.unwrap());
        // Deleting nonexistent file returns error
        let result = backend.delete(Path::new("nonexistent.txt")).await;
        let err = result.unwrap_err();
        assert!(matches!(&*err, ErrorKind::NotFound(_)));
    }

    #[tokio::test]
    async fn test_rename() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        backend.write(Path::new("old.txt"), b"data").await.unwrap();
        backend.rename(Path::new("old.txt"), Path::new("new.txt")).await.unwrap();
        assert!(!backend.exists(Path::new("old.txt")).await.unwrap());
        assert!(backend.exists(Path::new("new.txt")).await.unwrap());
        let data = backend.read(Path::new("new.txt")).await.unwrap();
        assert_eq!(data, b"data");
    }

    #[tokio::test]
    async fn test_rename_creates_directories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        backend.write(Path::new("file.txt"), b"data").await.unwrap();
        backend.rename(Path::new("file.txt"), Path::new("a/b/c/file.txt")).await.unwrap();
        assert!(backend.exists(Path::new("a/b/c/file.txt")).await.unwrap());
    }

    #[tokio::test]
    async fn test_stat() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        let data = b"Hello, world!";
        backend.write(Path::new("file.txt"), data).await.unwrap();
        let info = backend.stat(Path::new("file.txt")).await.unwrap();
        assert_eq!(info.path, PathBuf::from("file.txt"));
        assert_eq!(info.size, data.len() as u64);
        assert_eq!(info.compression, Compression::None);
        assert_eq!(info.file_hash, ());
    }

    #[tokio::test]
    async fn test_list_empty_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        let files = backend.list(None).await.unwrap();
        assert_eq!(files.len(), 0);
    }

    #[tokio::test]
    async fn test_list_returns_all_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        backend.write(Path::new("file.html"), b"data").await.unwrap();
        backend.write(Path::new("file.html.bz2"), b"data").await.unwrap();
        backend.write(Path::new("file.txt"), b"data").await.unwrap();
        backend.write(Path::new("README.md"), b"data").await.unwrap();
        let files = backend.list(None).await.unwrap();
        assert_eq!(files.len(), 4);
    }

    #[tokio::test]
    async fn test_list_with_prefix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        backend.write(Path::new("Fandom1/work1.html.bz2"), b"data").await.unwrap();
        backend.write(Path::new("Fandom1/work2.html.bz2"), b"data").await.unwrap();
        backend.write(Path::new("Fandom2/work3.html.bz2"), b"data").await.unwrap();
        let all_files = backend.list(None).await.unwrap();
        assert_eq!(all_files.len(), 3);
        let fandom1_files = backend.list(Some(Path::new("Fandom1/"))).await.unwrap();
        assert_eq!(fandom1_files.len(), 2);
        let paths: Vec<_> = fandom1_files.iter().map(|f| &f.path).collect();
        assert!(paths.contains(&&PathBuf::from("Fandom1/work1.html.bz2")));
        assert!(paths.contains(&&PathBuf::from("Fandom1/work2.html.bz2")));
    }

    #[tokio::test]
    async fn test_list_nonexistent_prefix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        let files = backend.list(Some(Path::new("nonexistent/"))).await.unwrap();
        assert_eq!(files.len(), 0);
    }

    #[tokio::test]
    async fn test_path_security() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        // Attempts to escape the root should fail
        assert!(backend.read(Path::new("../etc/passwd")).await.is_err());
        assert!(backend.read(Path::new("etc/../../passwd")).await.is_err());
        assert!(backend.write(Path::new("../etc/passwd"), b"data").await.is_err());
        assert!(backend.delete(Path::new("../../file")).await.is_err());
    }

    #[tokio::test]
    async fn test_reader() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        backend.write(Path::new("file.txt"), b"hello world").await.unwrap();
        let mut reader = backend.reader(Path::new("file.txt")).await.unwrap();
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"hello world");
    }

    #[tokio::test]
    async fn test_reader_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        let Err(err) = backend.reader(Path::new("missing.txt")).await else {
            panic!("expected NotFound error");
        };
        assert!(matches!(&*err, ErrorKind::NotFound(_)));
    }

    #[tokio::test]
    async fn test_writer() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path(), false).unwrap();
        let mut writer = backend.writer(Path::new("file.txt")).await.unwrap();
        writer.write_all(b"hello ").await.unwrap();
        writer.write_all(b"world").await.unwrap();
        writer.close().await.unwrap();
        let data = backend.read(Path::new("file.txt")).await.unwrap();
        assert_eq!(data, b"hello world");
    }
}
