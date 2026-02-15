//! In-memory storage backend for testing.

use super::FileInfoStream;
use crate::error::{ErrorKind, Result};
use crate::file::FileInfo;
use crate::path::validate as validate_path;
use async_stream::stream;
use async_trait::async_trait;
use rawr_compress::Compression;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use time::UtcDateTime;
use tokio::sync::RwLock;

use crate::StorageBackend;

/// In-memory storage backend for testing.
///
/// Files are stored in a `HashMap` behind a [`RwLock`], so all trait methods
/// can operate on `&self` without external synchronisation. Ideal for unit
/// tests that need a [`StorageBackend`] without filesystem or network
/// dependencies.
///
/// # Examples
///
/// ```
/// use rawr_storage::backend::{MockBackend, StorageBackend};
/// use std::path::Path;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = MockBackend::with_files([
///     ("works/123.html.gz", b"<html>...</html>"),
/// ]);
/// assert!(backend.exists(Path::new("works/123.html.gz")).await?);
///
/// backend.write(Path::new("works/321.html"), b"data...").await?;
/// assert!(backend.exists(Path::new("works/321.html")).await?);
/// # Ok(())
/// # }
/// ```
pub struct MockBackend {
    name: String,
    storage: RwLock<HashMap<PathBuf, (UtcDateTime, Vec<u8>)>>,
}

impl MockBackend {
    /// Create a mock backend pre-populated with files.
    ///
    /// Panics if any path fails validation (e.g. path traversal). If test
    /// setup is wrong, then test should not pass.
    ///
    /// # Example
    ///
    /// ```
    /// use rawr_storage::backend::MockBackend;
    ///
    /// let backend = MockBackend::with_files([
    ///     ("one.html", b"data file 1"),
    ///     ("dir/two.html", b"data file 2"),
    /// ]);
    /// ```
    pub fn with_files(files: impl IntoIterator<Item = (impl Into<PathBuf>, impl Into<Vec<u8>>)>) -> Self {
        let mut map = HashMap::new();
        let now = UtcDateTime::now();
        for (path, data) in files {
            let path = path.into();
            let Ok(validated) = validate_path(&path) else {
                // The panic here is DELIBERATE. MockBackend is intended to be
                // used in tests; panics are expected. There is no error result.
                panic!("MockBackend::with_files: invalid path {}", path.display());
            };
            map.insert(validated, (now, data.into()));
        }
        Self {
            name: "mock".to_string(),
            storage: RwLock::new(map),
        }
    }

    /// Change the name of the mock backend.
    ///
    /// # Example
    ///
    /// ```
    /// use rawr_storage::backend::MockBackend;
    ///
    /// let backend = MockBackend::default().with_name("test");
    /// ```
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    fn file_info(&self, path: &Path, size: u64, inserted: UtcDateTime) -> FileInfo {
        FileInfo::new(path, size, inserted, Compression::from_path(path))
    }
}
impl Default for MockBackend {
    fn default() -> Self {
        let files: [(&str, &str); 0] = [];
        Self::with_files(files)
    }
}

#[async_trait]
impl StorageBackend for MockBackend {
    fn name(&self) -> &str {
        &self.name
    }

    fn list_stream<'a>(&'a self, prefix: Option<&'a Path>) -> FileInfoStream<'a> {
        let validated_prefix = match prefix.map(validate_path).transpose() {
            Ok(pfx) => pfx,
            Err(e) => return Box::pin(futures::stream::once(async { Err(e) })),
        };

        Box::pin(stream! {
            // Snapshot matching entries under the read lock, then drop it
            // before yielding to avoid holding the lock across yield points.
            let entries: Vec<(PathBuf, (UtcDateTime, u64))> = {
                let guard = self.storage.read().await;
                guard
                    .iter()
                    .filter(|(path, _)| match &validated_prefix {
                        Some(pfx) => path.starts_with(pfx),
                        None => true,
                    })
                    .map(|(path, (inserted, data))| (path.clone(), (*inserted, data.len() as u64)))
                    .collect()
            };
            for (path, (inserted, size)) in entries {
                yield Ok(self.file_info(&path, size, inserted));
            }
        })
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        let path = validate_path(path)?;
        Ok(self.storage.read().await.contains_key(&path))
    }

    async fn read(&self, path: &Path) -> Result<Vec<u8>> {
        let path = validate_path(path)?;
        let (_inserted, data) =
            self.storage.read().await.get(&path).cloned().ok_or_else(|| exn::Exn::from(ErrorKind::NotFound(path)))?;
        Ok(data)
    }

    async fn read_head(&self, path: &Path, bytes: usize) -> Result<Vec<u8>> {
        let path = validate_path(path)?;
        let guard = self.storage.read().await;
        let (_inserted, data) = guard.get(&path).ok_or_else(|| exn::Exn::from(ErrorKind::NotFound(path.clone())))?;
        let end = bytes.min(data.len());
        Ok(data[..end].to_vec())
    }

    async fn write(&self, path: &Path, data: &[u8]) -> Result<()> {
        let path = validate_path(path)?;
        self.storage.write().await.insert(path, (UtcDateTime::now(), data.to_vec()));
        Ok(())
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        let path = validate_path(path)?;
        self.storage.write().await.remove(&path).map(|_| ()).ok_or_else(|| exn::Exn::from(ErrorKind::NotFound(path)))
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let from = validate_path(from)?;
        let to = validate_path(to)?;
        let mut guard = self.storage.write().await;
        let data = guard.remove(&from).ok_or_else(|| exn::Exn::from(ErrorKind::NotFound(from)))?;
        guard.insert(to, data);
        Ok(())
    }

    async fn stat(&self, path: &Path) -> Result<FileInfo> {
        let path = validate_path(path)?;
        let guard = self.storage.read().await;
        let (inserted, data) = guard.get(&path).ok_or_else(|| exn::Exn::from(ErrorKind::NotFound(path.clone())))?;
        Ok(self.file_info(&path, data.len() as u64, *inserted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_and_read() {
        let backend = MockBackend::default();
        backend.write(Path::new("test.txt"), b"hello").await.unwrap();
        let data = backend.read(Path::new("test.txt")).await.unwrap();
        assert_eq!(data, b"hello");
    }

    #[tokio::test]
    async fn test_with_files() {
        let backend = MockBackend::with_files([
            ("a/file.html.gz", Vec::from(*b"compressed")),
            ("b/file.html", Vec::from(*b"plain")),
        ]);
        assert!(backend.exists(Path::new("a/file.html.gz")).await.unwrap());
        assert!(backend.exists(Path::new("b/file.html")).await.unwrap());
        assert!(!backend.exists(Path::new("c/nope")).await.unwrap());
    }

    #[tokio::test]
    async fn test_read_not_found() {
        let backend = MockBackend::default();
        let err = backend.read(Path::new("missing.txt")).await.unwrap_err();
        assert!(matches!(&*err, ErrorKind::NotFound(_)));
    }

    #[tokio::test]
    async fn test_read_head() {
        let backend = MockBackend::default();
        backend.write(Path::new("file.txt"), b"0123456789").await.unwrap();
        let head = backend.read_head(Path::new("file.txt"), 4).await.unwrap();
        assert_eq!(head, b"0123");
        // More than file size returns everything
        let all = backend.read_head(Path::new("file.txt"), 100).await.unwrap();
        assert_eq!(all, b"0123456789");
    }

    #[tokio::test]
    async fn test_delete() {
        let backend = MockBackend::default();
        backend.write(Path::new("file.txt"), b"data").await.unwrap();
        backend.delete(Path::new("file.txt")).await.unwrap();
        assert!(!backend.exists(Path::new("file.txt")).await.unwrap());
        // Delete nonexistent → NotFound
        let err = backend.delete(Path::new("file.txt")).await.unwrap_err();
        assert!(matches!(&*err, ErrorKind::NotFound(_)));
    }

    #[tokio::test]
    async fn test_rename() {
        let backend = MockBackend::default();
        backend.write(Path::new("old.txt"), b"data").await.unwrap();
        backend.rename(Path::new("old.txt"), Path::new("new.txt")).await.unwrap();
        assert!(!backend.exists(Path::new("old.txt")).await.unwrap());
        assert_eq!(backend.read(Path::new("new.txt")).await.unwrap(), b"data");
    }

    #[tokio::test]
    async fn test_rename_not_found() {
        let backend = MockBackend::default();
        let err = backend.rename(Path::new("missing.txt"), Path::new("new.txt")).await.unwrap_err();
        assert!(matches!(&*err, ErrorKind::NotFound(_)));
    }

    #[tokio::test]
    async fn test_stat() {
        let backend = MockBackend::default();
        backend.write(Path::new("file.html.bz2"), b"12345").await.unwrap();
        let info = backend.stat(Path::new("file.html.bz2")).await.unwrap();
        assert_eq!(info.path, PathBuf::from("file.html.bz2"));
        assert_eq!(info.size, 5);
        assert_eq!(info.compression, Compression::Bzip2);
    }

    #[tokio::test]
    async fn test_list_with_prefix() {
        let backend = MockBackend::with_files([
            ("Fandom1/work1.html", Vec::from(*b"a")),
            ("Fandom1/work2.html", Vec::from(*b"b")),
            ("Fandom2/work3.html", Vec::from(*b"c")),
        ]);
        let files = backend.list(Some(Path::new("Fandom1"))).await.unwrap();
        assert_eq!(files.len(), 2);
        let paths: Vec<_> = files.iter().map(|f| &f.path).collect();
        assert!(paths.contains(&&PathBuf::from("Fandom1/work1.html")));
        assert!(paths.contains(&&PathBuf::from("Fandom1/work2.html")));
    }

    #[tokio::test]
    async fn test_list_all() {
        let backend = MockBackend::with_files([("a.txt", Vec::from(*b"1")), ("b.txt", Vec::from(*b"2"))]);
        let files = backend.list(None).await.unwrap();
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn test_path_traversal_rejected() {
        let backend = MockBackend::default();
        assert!(backend.read(Path::new("../etc/passwd")).await.is_err());
        assert!(backend.write(Path::new("../escape"), b"bad").await.is_err());
    }

    #[test]
    #[should_panic(expected = "invalid path")]
    fn test_with_files_panics_on_bad_path() {
        MockBackend::with_files([("../escape", Vec::from(*b"bad"))]);
    }
}
