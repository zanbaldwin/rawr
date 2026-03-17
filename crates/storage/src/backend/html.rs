//! HTML-filtered storage backend decorator.
//!
//! Wraps another backend and restricts all operations to files with
//! `.html` base extension (after stripping any compression suffix).

use crate::ValidPath;
use crate::backend::{BoxedReader, BoxedWriter, FileInfoStream, OperatorAware};
use crate::error::ErrorKind;
use crate::{BackendHandle, StorageBackend, error::Result, file::FileInfo};
use async_trait::async_trait;
use futures::StreamExt;
use opendal::Operator;
use rawr_compress::Compression;
use std::path::Path;

/// Check if a path has `.html` as its base extension.
///
/// Strips known compression suffixes first:
/// - `file.html` -> html -> true
/// - `file.html.bz2` -> strip .bz2 -> html -> true
/// - `file.txt` -> txt -> false
pub fn is_html_path(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    let compression = Compression::from_path(path);
    let check_path = if compression != Compression::None {
        // Strip the compression extension to get the inner filename
        Path::new(path.file_stem().unwrap_or_default())
    } else {
        path
    };
    check_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm"))
}

/// HTML-filtered storage backend.
///
/// Wraps another backend and restricts all operations to files with
/// `.html` base extension (with or without compression suffix).
/// Non-HTML paths return `ErrorKind::FilteredPath`.
#[derive(Clone)]
pub struct HtmlOnlyBackend {
    inner: BackendHandle,
}
impl HtmlOnlyBackend {
    pub fn new(inner: BackendHandle) -> Self {
        Self { inner }
    }
}
impl OperatorAware for HtmlOnlyBackend {
    fn operator(&self) -> &Operator {
        self.inner.operator()
    }
}
#[async_trait]
impl StorageBackend for HtmlOnlyBackend {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn list_stream<'a>(&'a self, prefix: Option<&'a ValidPath>) -> FileInfoStream<'a> {
        Box::pin(self.inner.list_stream(prefix).filter(|item| {
            std::future::ready(match item {
                Ok(info) => is_html_path(info.path.as_path()),
                Err(_) => true, // propagate errors
            })
        }))
    }

    async fn exists(&self, path: &ValidPath) -> Result<bool> {
        if !is_html_path(path.as_path()) {
            exn::bail!(ErrorKind::FilteredPath(path.to_string()));
        }
        self.inner.exists(path).await
    }

    async fn read(&self, path: &ValidPath) -> Result<Vec<u8>> {
        if !is_html_path(path.as_path()) {
            exn::bail!(ErrorKind::FilteredPath(path.to_string()));
        }
        self.inner.read(path).await
    }

    async fn read_head(&self, path: &ValidPath, bytes: usize) -> Result<Vec<u8>> {
        if !is_html_path(path.as_path()) {
            exn::bail!(ErrorKind::FilteredPath(path.to_string()));
        }
        self.inner.read_head(path, bytes).await
    }

    async fn write(&self, path: &ValidPath, data: &[u8]) -> Result<()> {
        if !is_html_path(path.as_path()) {
            exn::bail!(ErrorKind::FilteredPath(path.to_string()));
        }
        self.inner.write(path, data).await
    }

    async fn delete(&self, path: &ValidPath) -> Result<()> {
        if !is_html_path(path.as_path()) {
            exn::bail!(ErrorKind::FilteredPath(path.to_string()));
        }
        self.inner.delete(path).await
    }

    async fn rename(&self, from: &ValidPath, to: &ValidPath) -> Result<()> {
        if !is_html_path(from.as_path()) {
            exn::bail!(ErrorKind::FilteredPath(from.to_string()));
        }
        if !is_html_path(to.as_path()) {
            exn::bail!(ErrorKind::FilteredPath(to.to_string()));
        }
        self.inner.rename(from, to).await
    }

    async fn stat(&self, path: &ValidPath) -> Result<FileInfo> {
        if !is_html_path(path.as_path()) {
            exn::bail!(ErrorKind::FilteredPath(path.to_string()));
        }
        self.inner.stat(path).await
    }

    async fn reader(&self, path: &ValidPath) -> Result<BoxedReader> {
        if !is_html_path(path.as_path()) {
            exn::bail!(ErrorKind::FilteredPath(path.to_string()));
        }
        self.inner.reader(path).await
    }

    async fn writer(&self, path: &ValidPath) -> Result<BoxedWriter> {
        if !is_html_path(path.as_path()) {
            exn::bail!(ErrorKind::FilteredPath(path.to_string()));
        }
        self.inner.writer(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackendHandle, StorageBackend, backend::LocalBackend, error::ErrorKind};
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn test_is_html_path_plain_html() {
        assert!(is_html_path(Path::new("file.html")));
    }

    #[test]
    fn test_is_html_path_compressed_html() {
        assert!(is_html_path(Path::new("file.html.bz2")));
        assert!(is_html_path(Path::new("file.html.gz")));
    }

    #[test]
    fn test_is_html_path_nested_directory() {
        assert!(is_html_path(Path::new("Fandom/work.html.bz2")));
        assert!(is_html_path(Path::new("a/b/c/file.html")));
    }

    #[test]
    fn test_is_html_path_rejects_non_html() {
        assert!(!is_html_path(Path::new("file.txt")));
        assert!(!is_html_path(Path::new("README.md")));
        assert!(!is_html_path(Path::new("file.json")));
    }

    #[test]
    fn test_is_html_path_rejects_no_extension() {
        assert!(!is_html_path(Path::new("Makefile")));
        assert!(!is_html_path(Path::new(".hidden")));
    }

    /// Helper: create a temp HtmlBackend wrapping a LocalBackend.
    fn setup() -> (tempfile::TempDir, HtmlOnlyBackend) {
        let temp_dir = tempfile::tempdir().unwrap();
        let local = LocalBackend::new("test", temp_dir.path().to_str().unwrap(), false).unwrap();
        let backend: BackendHandle = Arc::new(local);
        let html = HtmlOnlyBackend::new(backend);
        (temp_dir, html)
    }

    #[tokio::test]
    async fn test_list_filters_by_extension() {
        let (dir, backend) = setup();
        backend.write(&ValidPath::new("file.html").unwrap(), b"data").await.unwrap();
        backend.write(&ValidPath::new("file.html.bz2").unwrap(), b"data").await.unwrap();
        // Write non-html files directly to filesystem (HtmlBackend gates writes)
        std::fs::write(dir.path().join("file.txt"), b"data").unwrap();
        std::fs::write(dir.path().join("README.md"), b"data").unwrap();

        let files = backend.list(None).await.unwrap();
        assert_eq!(files.len(), 2);
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"file.html"));
        assert!(paths.contains(&"file.html.bz2"));
    }

    #[tokio::test]
    async fn test_read_rejects_non_html() {
        let (_dir, backend) = setup();
        let result = backend.read(&ValidPath::new("file.txt").unwrap()).await;
        let err = result.unwrap_err();
        assert!(matches!(&*err, ErrorKind::FilteredPath(_)));
    }

    #[tokio::test]
    async fn test_write_rejects_non_html() {
        let (_dir, backend) = setup();
        let result = backend.write(&ValidPath::new("file.txt").unwrap(), b"data").await;
        let err = result.unwrap_err();
        assert!(matches!(&*err, ErrorKind::FilteredPath(_)));
    }

    #[tokio::test]
    async fn test_exists_rejects_non_html() {
        let (_dir, backend) = setup();
        let result = backend.exists(&ValidPath::new("file.txt").unwrap()).await;
        let err = result.unwrap_err();
        assert!(matches!(&*err, ErrorKind::FilteredPath(_)));
    }

    #[tokio::test]
    async fn test_rename_validates_both_paths() {
        let (_dir, backend) = setup();
        // html -> non-html: should fail on `to`
        backend.write(&ValidPath::new("a.html").unwrap(), b"data").await.unwrap();
        let result = backend.rename(&ValidPath::new("a.html").unwrap(), &ValidPath::new("a.txt").unwrap()).await;
        assert!(matches!(&*result.unwrap_err(), ErrorKind::FilteredPath(_)));
        // non-html -> html: should fail on `from`
        let result = backend.rename(&ValidPath::new("a.txt").unwrap(), &ValidPath::new("b.html").unwrap()).await;
        assert!(matches!(&*result.unwrap_err(), ErrorKind::FilteredPath(_)));
        // html -> html: should succeed
        backend.rename(&ValidPath::new("a.html").unwrap(), &ValidPath::new("b.html").unwrap()).await.unwrap();
    }
}
