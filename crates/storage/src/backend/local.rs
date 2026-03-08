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
    pub fn new(name: impl Into<String>, root: impl AsRef<str>, auto_create: bool) -> Result<Self> {
        let root_str = root.as_ref();
        let root_path = Path::new(root_str);
        if !root_path.is_absolute() {
            exn::bail!(ErrorKind::InvalidPath(root_str.to_string()));
        }
        if root_path.exists() {
            if !root_path.is_dir() {
                exn::bail!(ErrorKind::InvalidPath(root_str.to_string()));
            }
        } else if auto_create {
            // Use non-async here; it'll only happen once on library initialization
            // and it's not worth the hassle of making the constructor async.
            sync_create_dir(&root_path).map_err(ErrorKind::Io)?;
        } else {
            exn::bail!(ErrorKind::PermissionDenied(root_str.to_string()));
        }

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
    use super::*;
    use crate::ValidatedPath;
    use crate::error::ErrorKind;
    use futures::io::{AsyncReadExt, AsyncWriteExt};
    use rawr_compress::Compression;

    #[test]
    fn test_new_requires_absolute_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert!(LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).is_ok());
        assert!(LocalBackend::new("name", "relative/path", false).is_err());
        assert!(LocalBackend::new("name", "./relative", false).is_err());
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        let data = b"Hello, world!";
        backend.write(&ValidatedPath::new("test.txt").unwrap(), data).await.unwrap();
        let read_data = backend.read(&ValidatedPath::new("test.txt").unwrap()).await.unwrap();
        assert_eq!(read_data, data);
    }

    #[tokio::test]
    async fn test_prefix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        let data = b"Hello, world!";
        backend.write(&ValidatedPath::new("FandomA/Sub/file.html").unwrap(), data).await.unwrap();
        backend.write(&ValidatedPath::new("FandomA/Subdir/file.html").unwrap(), data).await.unwrap();
        backend.write(&ValidatedPath::new("FandomA/Subfile.html").unwrap(), data).await.unwrap();
        let mut files = backend.list(Some(&ValidatedPath::new("FandomA/Sub").unwrap())).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(&files.pop().unwrap().path, "FandomA/Sub/file.html");
    }

    #[tokio::test]
    async fn test_write_creates_directories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        backend.write(&ValidatedPath::new("a/b/c/file.txt").unwrap(), b"data").await.unwrap();
        assert!(backend.exists(&ValidatedPath::new("a/b/c/file.txt").unwrap()).await.unwrap());
    }

    #[tokio::test]
    async fn test_exists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        assert!(!backend.exists(&ValidatedPath::new("nonexistent.txt").unwrap()).await.unwrap());
        backend.write(&ValidatedPath::new("exists.txt").unwrap(), b"data").await.unwrap();
        assert!(backend.exists(&ValidatedPath::new("exists.txt").unwrap()).await.unwrap());
    }

    #[tokio::test]
    async fn test_read_head() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        let data = b"0123456789ABCDEF";
        backend.write(&ValidatedPath::new("file.txt").unwrap(), data).await.unwrap();
        let head = backend.read_head(&ValidatedPath::new("file.txt").unwrap(), 5).await.unwrap();
        assert_eq!(head, b"01234");
        // Reading more bytes than file size returns entire file
        let all = backend.read_head(&ValidatedPath::new("file.txt").unwrap(), 100).await.unwrap();
        assert_eq!(all, data);
    }

    #[tokio::test]
    async fn test_delete() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        backend.write(&ValidatedPath::new("file.txt").unwrap(), b"data").await.unwrap();
        assert!(backend.exists(&ValidatedPath::new("file.txt").unwrap()).await.unwrap());
        backend.delete(&ValidatedPath::new("file.txt").unwrap()).await.unwrap();
        assert!(!backend.exists(&ValidatedPath::new("file.txt").unwrap()).await.unwrap());
        // Deleting nonexistent file returns error
        let result = backend.delete(&ValidatedPath::new("nonexistent.txt").unwrap()).await;
        let err = result.unwrap_err();
        assert!(matches!(&*err, ErrorKind::NotFound(_)));
    }

    #[tokio::test]
    async fn test_rename() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        backend.write(&ValidatedPath::new("old.txt").unwrap(), b"data").await.unwrap();
        backend
            .rename(&ValidatedPath::new("old.txt").unwrap(), &ValidatedPath::new("new.txt").unwrap())
            .await
            .unwrap();
        assert!(!backend.exists(&ValidatedPath::new("old.txt").unwrap()).await.unwrap());
        assert!(backend.exists(&ValidatedPath::new("new.txt").unwrap()).await.unwrap());
        let data = backend.read(&ValidatedPath::new("new.txt").unwrap()).await.unwrap();
        assert_eq!(data, b"data");
    }

    #[tokio::test]
    async fn test_rename_creates_directories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        backend.write(&ValidatedPath::new("file.txt").unwrap(), b"data").await.unwrap();
        backend
            .rename(&ValidatedPath::new("file.txt").unwrap(), &ValidatedPath::new("a/b/c/file.txt").unwrap())
            .await
            .unwrap();
        assert!(backend.exists(&ValidatedPath::new("a/b/c/file.txt").unwrap()).await.unwrap());
    }

    #[tokio::test]
    async fn test_stat() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        let data = b"Hello, world!";
        backend.write(&ValidatedPath::new("file.txt").unwrap(), data).await.unwrap();
        let info = backend.stat(&ValidatedPath::new("file.txt").unwrap()).await.unwrap();
        assert_eq!(&info.path, "file.txt");
        assert_eq!(info.size, data.len() as u64);
        assert_eq!(info.compression, Compression::None);
        assert_eq!(info.file_hash, ());
    }

    #[tokio::test]
    async fn test_list_empty_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        let files = backend.list(None).await.unwrap();
        assert_eq!(files.len(), 0);
    }

    #[tokio::test]
    async fn test_list_returns_all_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        backend.write(&ValidatedPath::new("file.html").unwrap(), b"data").await.unwrap();
        backend.write(&ValidatedPath::new("file.html.bz2").unwrap(), b"data").await.unwrap();
        backend.write(&ValidatedPath::new("file.txt").unwrap(), b"data").await.unwrap();
        backend.write(&ValidatedPath::new("README.md").unwrap(), b"data").await.unwrap();
        let files = backend.list(None).await.unwrap();
        assert_eq!(files.len(), 4);
    }

    #[tokio::test]
    async fn test_list_with_prefix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        backend.write(&ValidatedPath::new("Fandom1/work1.html.bz2").unwrap(), b"data").await.unwrap();
        backend.write(&ValidatedPath::new("Fandom1/work2.html.bz2").unwrap(), b"data").await.unwrap();
        backend.write(&ValidatedPath::new("Fandom2/work3.html.bz2").unwrap(), b"data").await.unwrap();
        let all_files = backend.list(None).await.unwrap();
        assert_eq!(all_files.len(), 3);
        let fandom1_files = backend.list(Some(&ValidatedPath::new("Fandom1/").unwrap())).await.unwrap();
        assert_eq!(fandom1_files.len(), 2);
        let paths: Vec<_> = fandom1_files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"Fandom1/work1.html.bz2"));
        assert!(paths.contains(&"Fandom1/work2.html.bz2"));
    }

    #[tokio::test]
    async fn test_list_nonexistent_prefix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        let files = backend.list(Some(&ValidatedPath::new("nonexistent/").unwrap())).await.unwrap();
        assert_eq!(files.len(), 0);
    }

    #[tokio::test]
    async fn test_reader() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        backend.write(&ValidatedPath::new("file.txt").unwrap(), b"hello world").await.unwrap();
        let mut reader = backend.reader(&ValidatedPath::new("file.txt").unwrap()).await.unwrap();
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"hello world");
    }

    #[tokio::test]
    async fn test_reader_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        let Err(err) = backend.reader(&ValidatedPath::new("missing.txt").unwrap()).await else {
            panic!("expected NotFound error");
        };
        assert!(matches!(&*err, ErrorKind::NotFound(_)));
    }

    #[tokio::test]
    async fn test_writer() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path().to_str().unwrap(), false).unwrap();
        let mut writer = backend.writer(&ValidatedPath::new("file.txt").unwrap()).await.unwrap();
        writer.write_all(b"hello ").await.unwrap();
        writer.write_all(b"world").await.unwrap();
        writer.close().await.unwrap();
        let data = backend.read(&ValidatedPath::new("file.txt").unwrap()).await.unwrap();
        assert_eq!(data, b"hello world");
    }
}
