//! HTML-filtered storage backend decorator.
//!
//! Wraps another backend and restricts all operations to files with
//! `.html` base extension (after stripping any compression suffix).

use crate::backend::{BoxSyncRead, BoxSyncWrite, FileInfoStream};
use crate::error::ErrorKind;
use crate::{BackendHandle, StorageBackend, error::Result, file::FileInfo};
use async_trait::async_trait;
use futures::StreamExt;
use rawr_compress::Compression;
use std::path::Path;

/// The required base extension (after stripping compression).
const HTML_EXTENSION: &str = "html";

/// Check if a path has `.html` as its base extension.
///
/// Strips known compression suffixes first:
/// - `file.html` -> html -> true
/// - `file.html.bz2` -> strip .bz2 -> html -> true
/// - `file.txt` -> txt -> false
fn is_html_path(path: impl AsRef<Path>) -> bool {
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
        .is_some_and(|ext| ext.eq_ignore_ascii_case(HTML_EXTENSION))
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

#[async_trait]
impl StorageBackend for HtmlOnlyBackend {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn list_stream<'a>(&'a self, prefix: Option<&'a Path>) -> FileInfoStream<'a> {
        Box::pin(self.inner.list_stream(prefix).filter(|item| {
            std::future::ready(match item {
                Ok(info) => is_html_path(&info.path),
                Err(_) => true, // propagate errors
            })
        }))
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        if !is_html_path(path) {
            exn::bail!(ErrorKind::FilteredPath(path.to_path_buf()));
        }
        self.inner.exists(path).await
    }

    async fn read(&self, path: &Path) -> Result<Vec<u8>> {
        if !is_html_path(path) {
            exn::bail!(ErrorKind::FilteredPath(path.to_path_buf()));
        }
        self.inner.read(path).await
    }

    async fn read_head(&self, path: &Path, bytes: usize) -> Result<Vec<u8>> {
        if !is_html_path(path) {
            exn::bail!(ErrorKind::FilteredPath(path.to_path_buf()));
        }
        self.inner.read_head(path, bytes).await
    }

    async fn reader(&self, path: &Path) -> Result<BoxSyncRead> {
        if !is_html_path(path) {
            exn::bail!(ErrorKind::FilteredPath(path.to_path_buf()));
        }
        self.inner.reader(path).await
    }

    async fn write(&self, path: &Path, data: &[u8]) -> Result<()> {
        if !is_html_path(path) {
            exn::bail!(ErrorKind::FilteredPath(path.to_path_buf()));
        }
        self.inner.write(path, data).await
    }

    async fn writer(&self, path: &Path) -> Result<BoxSyncWrite> {
        if !is_html_path(path) {
            exn::bail!(ErrorKind::FilteredPath(path.to_path_buf()));
        }
        self.inner.writer(path).await
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        if !is_html_path(path) {
            exn::bail!(ErrorKind::FilteredPath(path.to_path_buf()));
        }
        self.inner.delete(path).await
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        if !is_html_path(from) {
            exn::bail!(ErrorKind::FilteredPath(from.to_path_buf()));
        }
        if !is_html_path(to) {
            exn::bail!(ErrorKind::FilteredPath(to.to_path_buf()));
        }
        self.inner.rename(from, to).await
    }

    async fn stat(&self, path: &Path) -> Result<FileInfo> {
        if !is_html_path(path) {
            exn::bail!(ErrorKind::FilteredPath(path.to_path_buf()));
        }
        self.inner.stat(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackendHandle, StorageBackend, backend::LocalBackend, error::ErrorKind};
    use std::path::Path;
    use std::path::PathBuf;
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
        let local = LocalBackend::new("test", temp_dir.path()).unwrap();
        let backend: BackendHandle = Arc::new(local);
        let html = HtmlOnlyBackend::new(backend);
        (temp_dir, html)
    }

    #[tokio::test]
    async fn test_list_filters_by_extension() {
        let (dir, backend) = setup();
        backend.write(Path::new("file.html"), b"data").await.unwrap();
        backend.write(Path::new("file.html.bz2"), b"data").await.unwrap();
        // Write non-html files directly to filesystem (HtmlBackend gates writes)
        std::fs::write(dir.path().join("file.txt"), b"data").unwrap();
        std::fs::write(dir.path().join("README.md"), b"data").unwrap();

        let files = backend.list(None).await.unwrap();
        assert_eq!(files.len(), 2);
        let paths: Vec<_> = files.iter().map(|f| &f.path).collect();
        assert!(paths.contains(&&PathBuf::from("file.html")));
        assert!(paths.contains(&&PathBuf::from("file.html.bz2")));
    }

    #[tokio::test]
    async fn test_read_rejects_non_html() {
        let (_dir, backend) = setup();
        let result = backend.read(Path::new("file.txt")).await;
        let err = result.unwrap_err();
        assert!(matches!(&*err, ErrorKind::FilteredPath(_)));
    }

    #[tokio::test]
    async fn test_write_rejects_non_html() {
        let (_dir, backend) = setup();
        let result = backend.write(Path::new("file.txt"), b"data").await;
        let err = result.unwrap_err();
        assert!(matches!(&*err, ErrorKind::FilteredPath(_)));
    }

    #[tokio::test]
    async fn test_exists_rejects_non_html() {
        let (_dir, backend) = setup();
        let result = backend.exists(Path::new("file.txt")).await;
        let err = result.unwrap_err();
        assert!(matches!(&*err, ErrorKind::FilteredPath(_)));
    }

    #[tokio::test]
    async fn test_rename_validates_both_paths() {
        let (_dir, backend) = setup();
        // html -> non-html: should fail on `to`
        backend.write(Path::new("a.html"), b"data").await.unwrap();
        let result = backend.rename(Path::new("a.html"), Path::new("a.txt")).await;
        assert!(matches!(&*result.unwrap_err(), ErrorKind::FilteredPath(_)));
        // non-html -> html: should fail on `from`
        let result = backend.rename(Path::new("a.txt"), Path::new("b.html")).await;
        assert!(matches!(&*result.unwrap_err(), ErrorKind::FilteredPath(_)));
        // html -> html: should succeed
        backend.rename(Path::new("a.html"), Path::new("b.html")).await.unwrap();
    }

    #[tokio::test]
    async fn test_reader_rejects_non_html() {
        let (_dir, backend) = setup();
        let Err(err) = backend.reader(Path::new("file.txt")).await else {
            panic!("expected error");
        };
        assert!(matches!(&*err, ErrorKind::FilteredPath(_)));
    }

    #[tokio::test]
    async fn test_writer_rejects_non_html() {
        let (_dir, backend) = setup();
        let Err(err) = backend.writer(Path::new("file.txt")).await else {
            panic!("expected error");
        };
        assert!(matches!(&*err, ErrorKind::FilteredPath(_)));
    }
}
