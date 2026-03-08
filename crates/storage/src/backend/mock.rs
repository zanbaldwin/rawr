//! In-memory storage backend for testing.

use super::map_opendal_error;
use crate::StorageBackend;
use crate::ValidatedPath;
use crate::backend::OperatorAware;
use crate::error::{ErrorKind, Result};
use async_trait::async_trait;
use futures::AsyncWriteExt;
use futures::io::copy as async_copy;
use opendal::Operator;
use opendal::services::Memory;
use std::path::Path;
use std::{fs::File, io::Read};

/// In-memory storage backend for testing.
///
/// Files are stored in an OpenDAL [`Memory`] operator, providing the same
/// interface as local/S3 backends without filesystem or network dependencies.
/// Ideal for unit tests that need a [`StorageBackend`].
///
/// # Examples
///
/// ```
/// use rawr_storage::backend::{MockBackend, StorageBackend};
/// use std::path::Path;
///
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = MockBackend::with_data([
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
    operator: Operator,
}
impl MockBackend {
    fn new_operator() -> Operator {
        Operator::new(Memory::default()).expect("Memory operator construction is infallible").finish()
    }

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
    /// let backend = MockBackend::with_data([
    ///     ("one.html", b"data file 1"),
    ///     ("dir/two.html", b"data file 2"),
    /// ]);
    /// ```
    pub fn with_data(files: impl IntoIterator<Item = (impl AsRef<Path>, impl Into<Vec<u8>>)>) -> Self {
        let operator = Self::new_operator();
        let blocking = operator.blocking();
        for (path, data) in files {
            let Ok(validated_path) = ValidatedPath::new(path.as_ref()) else {
                // The panic here is DELIBERATE. MockBackend is intended to be
                // used in tests; panics are expected. There is no error result.
                panic!("MockBackend::with_data(): invalid path {}", path.as_ref().display());
            };
            let Ok(_) = blocking.write(validated_path.as_str(), data.into()) else {
                panic!("MockBackend::with_data(): could not write data to path {}", path.as_ref().display());
            };
        }
        Self { name: "mock".to_string(), operator }
    }

    /// Mock storage backend from real test fixtures.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use rawr_storage::backend::MockBackend;
    ///
    /// let backend = MockBackend::from_files([
    ///     "../../tests/fixtures/work1.html",
    ///     "../../tests/fixtures/work2.html",
    /// ]);
    /// ```
    pub fn from_files<P: AsRef<Path>>(files: impl IntoIterator<Item = P>) -> Self {
        let operator = Self::new_operator();
        let blocking = operator.blocking();
        for path in files {
            let path = path.as_ref();
            if !path.exists() {
                panic!("MockBackend::with_files(): file does not exist {}", path.display());
            }
            let mut file = File::open(path).unwrap();
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).unwrap();
            // Place files directly into the root of the storage backend,
            // instead of needing to validating custom locations.
            let filename = path.file_name().unwrap().to_str().unwrap();
            blocking.write(filename, contents).unwrap();
        }
        Self { name: "mock".to_string(), operator }
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
}
impl Default for MockBackend {
    fn default() -> Self {
        Self {
            name: "mock".to_string(),
            operator: Self::new_operator(),
        }
    }
}

impl OperatorAware for MockBackend {
    fn operator(&self) -> &Operator {
        &self.operator
    }
}
#[async_trait]
impl StorageBackend for MockBackend {
    fn name(&self) -> &str {
        &self.name
    }

    // Memory service doesn't support rename natively — use copy+delete.
    async fn rename(&self, from: &ValidatedPath, to: &ValidatedPath) -> Result<()> {
        if !self.exists(from).await? {
            exn::bail!(ErrorKind::NotFound(from.to_string()));
        }
        let mut reader = self.reader(from).await?;
        let mut writer = self.writer(to).await?;
        async_copy(&mut reader, &mut writer).await.map_err(ErrorKind::Io)?;
        writer.close().await.map_err(ErrorKind::Io)?;
        self.operator.delete(from.as_str()).await.map_err(|e| map_opendal_error(e, from.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use futures::io::{AsyncReadExt, AsyncWriteExt};
    use rawr_compress::Compression;

    #[tokio::test]
    async fn test_write_and_read() {
        let backend = MockBackend::default();
        backend.write(&ValidatedPath::new("test.txt").unwrap(), b"hello").await.unwrap();
        let data = backend.read(&ValidatedPath::new("test.txt").unwrap()).await.unwrap();
        assert_eq!(data, b"hello");
    }

    #[tokio::test]
    async fn test_with_files() {
        let backend = MockBackend::with_data([
            ("a/file.html.gz", Vec::from(*b"compressed")),
            ("b/file.html", Vec::from(*b"plain")),
        ]);
        assert!(backend.exists(&ValidatedPath::new("a/file.html.gz").unwrap()).await.unwrap());
        assert!(backend.exists(&ValidatedPath::new("b/file.html").unwrap()).await.unwrap());
        assert!(!backend.exists(&ValidatedPath::new("c/nope").unwrap()).await.unwrap());
    }

    #[tokio::test]
    async fn test_read_not_found() {
        let backend = MockBackend::default();
        let err = backend.read(&ValidatedPath::new("missing.txt").unwrap()).await.unwrap_err();
        assert!(matches!(&*err, ErrorKind::NotFound(_)));
    }

    #[tokio::test]
    async fn test_read_head() {
        let backend = MockBackend::default();
        backend.write(&ValidatedPath::new("file.txt").unwrap(), b"0123456789").await.unwrap();
        let head = backend.read_head(&ValidatedPath::new("file.txt").unwrap(), 4).await.unwrap();
        assert_eq!(head, b"0123");
        // More than file size returns everything
        let all = backend.read_head(&ValidatedPath::new("file.txt").unwrap(), 100).await.unwrap();
        assert_eq!(all, b"0123456789");
    }

    #[tokio::test]
    async fn test_delete() {
        let backend = MockBackend::default();
        backend.write(&ValidatedPath::new("file.txt").unwrap(), b"data").await.unwrap();
        backend.delete(&ValidatedPath::new("file.txt").unwrap()).await.unwrap();
        assert!(!backend.exists(&ValidatedPath::new("file.txt").unwrap()).await.unwrap());
        // Delete nonexistent → NotFound
        let err = backend.delete(&ValidatedPath::new("file.txt").unwrap()).await.unwrap_err();
        assert!(matches!(&*err, ErrorKind::NotFound(_)));
    }

    #[tokio::test]
    async fn test_rename() {
        let backend = MockBackend::default();
        backend.write(&ValidatedPath::new("old.txt").unwrap(), b"data").await.unwrap();
        backend
            .rename(&ValidatedPath::new("old.txt").unwrap(), &ValidatedPath::new("new.txt").unwrap())
            .await
            .unwrap();
        assert!(!backend.exists(&ValidatedPath::new("old.txt").unwrap()).await.unwrap());
        assert_eq!(backend.read(&ValidatedPath::new("new.txt").unwrap()).await.unwrap(), b"data");
    }

    #[tokio::test]
    async fn test_rename_not_found() {
        let backend = MockBackend::default();
        let err = backend
            .rename(&ValidatedPath::new("missing.txt").unwrap(), &ValidatedPath::new("new.txt").unwrap())
            .await
            .unwrap_err();
        assert!(matches!(&*err, ErrorKind::NotFound(_)));
    }

    #[tokio::test]
    async fn test_stat() {
        let backend = MockBackend::default();
        backend.write(&ValidatedPath::new("file.html.bz2").unwrap(), b"12345").await.unwrap();
        let info = backend.stat(&ValidatedPath::new("file.html.bz2").unwrap()).await.unwrap();
        assert_eq!(&info.path, "file.html.bz2");
        assert_eq!(info.size, 5);
        assert_eq!(info.compression, Compression::Bzip2);
        assert_eq!(info.file_hash, ());
    }

    #[tokio::test]
    async fn test_list_with_prefix() {
        let backend = MockBackend::with_data([
            ("Fandom1/work1.html", Vec::from(*b"a")),
            ("Fandom1/work2.html", Vec::from(*b"b")),
            ("Fandom2/work3.html", Vec::from(*b"c")),
        ]);
        let files = backend.list(Some(&ValidatedPath::new("Fandom1").unwrap())).await.unwrap();
        assert_eq!(files.len(), 2);
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"Fandom1/work1.html"));
        assert!(paths.contains(&"Fandom1/work2.html"));
    }

    #[tokio::test]
    async fn test_list_all() {
        let backend = MockBackend::with_data([("a.txt", Vec::from(*b"1")), ("b.txt", Vec::from(*b"2"))]);
        let files = backend.list(None).await.unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    #[should_panic(expected = "invalid path")]
    fn test_with_files_panics_on_bad_path() {
        MockBackend::with_data([("../escape", Vec::from(*b"bad"))]);
    }

    #[tokio::test]
    async fn test_reader() {
        let backend = MockBackend::with_data([("file.txt", Vec::from(*b"hello world"))]);
        let mut reader = backend.reader(&ValidatedPath::new("file.txt").unwrap()).await.unwrap();
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"hello world");
    }

    #[tokio::test]
    async fn test_reader_not_found() {
        let backend = MockBackend::default();
        let Err(err) = backend.reader(&ValidatedPath::new("missing.txt").unwrap()).await else {
            panic!("expected NotFound error");
        };
        assert!(matches!(&*err, ErrorKind::NotFound(_)));
    }

    #[tokio::test]
    async fn test_writer() {
        let backend = MockBackend::default();
        let mut writer = backend.writer(&ValidatedPath::new("file.txt").unwrap()).await.unwrap();
        writer.write_all(b"hello ").await.unwrap();
        writer.write_all(b"world").await.unwrap();
        writer.close().await.unwrap();
        let data = backend.read(&ValidatedPath::new("file.txt").unwrap()).await.unwrap();
        assert_eq!(data, b"hello world");
    }
}
