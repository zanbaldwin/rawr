//! Local filesystem storage backend.
//!
//! This module provides a storage backend implementation for the local filesystem.
//! Files are stored in a configured directory and accessed using standard filesystem
//! operations via `tokio::fs` for async I/O.

use crate::backend::FileInfoStream;
use crate::error::ErrorKind;
use crate::{FileInfo, StorageBackend, error::Result, path::validate as validate_path};
use async_stream::stream;
use async_trait::async_trait;
use exn::ResultExt;
use rawr_compress::Compression;
use std::fs::{Metadata, create_dir_all as sync_create_dir};
use std::path::{Path, PathBuf};
use tokio::fs::{self, DirEntry};
use tokio::io::AsyncReadExt;

enum WalkEntry {
    File(FileInfo),
    Descend(PathBuf),
    Skip,
}

/// Local filesystem storage backend.
///
/// Stores files in a directory on the local filesystem. All paths are relative
/// to the configured root directory.
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
#[derive(Clone)]
pub struct LocalBackend {
    name: String,
    /// Root directory for the library
    root: PathBuf,
}
impl LocalBackend {
    /// Create a new local filesystem backend.
    ///
    /// # Arguments
    /// * `root` - Absolute path to the library root directory
    ///
    /// # Errors
    ///
    /// Returns an error if the path is not absolute.
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
    pub fn new(name: impl Into<String>, root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        if !root.is_absolute() {
            exn::bail!(ErrorKind::InvalidPath(root));
        }

        // TODO: What if this is decorated by ReadonlyBackend? Is it even possible to detect that?
        if root.exists() {
            if !root.is_dir() {
                exn::bail!(ErrorKind::InvalidPath(root));
            }
        } else {
            // Use non-async here; it'll only happen once on library initialization
            // and it's not worth the hassle of making the constructor async.
            sync_create_dir(&root).map_err(|e| Self::map_io_error(e, &root))?;
        }

        Ok(Self { name: name.into(), root })
    }

    /// Get the absolute path for a relative storage path.
    ///
    /// Validates the path and joins it with the root directory.
    fn absolute_path(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let validated = validate_path(path.as_ref())?;
        Ok(self.root.join(validated))
    }

    /// Convert an absolute path back to a relative storage path.
    ///
    /// Strips the root prefix and converts to a string.
    fn relative_path(&self, absolute: impl AsRef<Path>) -> Result<PathBuf> {
        let absolute = absolute.as_ref();
        if !absolute.is_absolute() {
            exn::bail!(ErrorKind::BackendError(format!(
                "attempting to get relative path of non-absolute path `{:?}`",
                absolute
            )))
        }
        let relative = absolute.strip_prefix(&self.root).or_raise(|| {
            ErrorKind::BackendError(format!("path `{:?}` is not within root `{:?}`", absolute, self.root))
        })?;
        // Validate path will also canonicalize it.
        Ok(validate_path(relative)?)
    }

    /// Re-use same data collection from file metadata for both list and stat functions
    fn metadata(path: &Path, metadata: Metadata) -> Result<FileInfo> {
        let modified = metadata.modified().map_err(ErrorKind::Io)?.into();
        let compression = Compression::from_path(path);
        Ok(FileInfo::new(PathBuf::from(path), metadata.len(), modified, compression))
    }

    fn map_io_error(e: std::io::Error, path: &Path) -> ErrorKind {
        match e.kind() {
            std::io::ErrorKind::NotFound => ErrorKind::NotFound(path.to_path_buf()),
            std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied(path.to_path_buf()),
            _ => ErrorKind::Io(e),
        }
    }

    /// Writing this helper function is the only way I could find to stay sane
    /// inside that stream loop where you can't `?` errors. You have to convert
    /// them to the right type, yield them, then continue the loop. It was
    /// driving me crazy.
    async fn process_entry(&self, entry: DirEntry, prefix: Option<&Path>) -> Result<WalkEntry> {
        let path = entry.path();
        let metadata = entry.metadata().await.map_err(|e| Self::map_io_error(e, &path))?;
        let relative = self.relative_path(&path)?;
        if let Some(pfx) = prefix
            && !relative.starts_with(pfx)
        {
            return Ok(WalkEntry::Skip);
        }
        if metadata.is_dir() {
            return Ok(WalkEntry::Descend(path));
        }
        if metadata.is_file() {
            return Ok(WalkEntry::File(Self::metadata(&relative, metadata)?));
        }
        // Note: silently drop what is most likely a broken symlink.
        Ok(WalkEntry::Skip)
    }
}

#[async_trait]
impl StorageBackend for LocalBackend {
    fn name(&self) -> &str {
        &self.name
    }

    // I know two things:
    // 1. Async streams are really hard to wrap my head around, and
    // 2. I do not know of a better way to get performant code.
    // Did I mention that async streams are really fucking hard?!
    fn list_stream<'a>(&'a self, prefix: Option<&'a Path>) -> FileInfoStream<'a> {
        let validated_prefix = match prefix.map(validate_path).transpose() {
            Ok(pfx) => pfx,
            // TODO: or.raise() to its own prefix error instead of "this
            //       generic path that could be about a listed file".
            Err(e) => return Box::pin(futures::stream::once(async { Result::Err(e) })),
        };

        let start_dir = validated_prefix
            .as_ref()
            // Walk from the parent directory of the prefix path. Ensures
            // prefix is a directory and avoids erroring on prefixes where
            // the leaf component doesn't exist yet or is a file.
            // So the prefix "FandomA/Sub" would become a starting
            // directory of "FandomA" and match:
            // - [MATCH] "FandomA/Sub/file.html"
            // - [MATCH] "FandomA/Sub" (could be file)
            // - [NOT MATCH] "FandomA/Subdir/file.html" (Path::starts_with is component-based)
            .map(|prefix| self.root.join(prefix).parent().unwrap_or_else(|| &self.root).to_path_buf())
            .unwrap_or_else(|| self.root.clone());
        let mut stack = vec![start_dir];

        Box::pin(stream! {
            'dirs: while let Some(current) = stack.pop() {
                let mut entries = match fs::read_dir(&current).await {
                    Ok(entries) => entries,
                    // To stay consistent with the behaviour of S3-compatible
                    // backends, asking for the contents of a directory that
                    // doesn't exist results in an empty list not an error.
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(err) => {
                        yield Err(exn::Exn::from(Self::map_io_error(err, &current)));
                        continue 'dirs;
                    }
                };

                'entries: loop {
                    let entry = match entries.next_entry().await {
                        Ok(Some(entry)) => entry,
                        Ok(None) => break 'entries,
                        // This stupid error? These were littered all over this
                        // function before I extracted the main logic out to
                        // Self::process_entry. It was hell.
                        Err(e) => { yield Err(exn::Exn::from(Self::map_io_error(e, &current))); continue 'entries; },
                    };
                    match self.process_entry(entry, validated_prefix.as_deref()).await {
                        Ok(WalkEntry::File(f)) => yield Ok(f),
                        Ok(WalkEntry::Descend(d)) => stack.push(d),
                        Ok(WalkEntry::Skip) => {},
                        Err(e) => yield Err(e),
                    };
                }
            }
        })
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        let abs_path = self.absolute_path(path)?;
        Ok(fs::try_exists(&abs_path).await.map_err(ErrorKind::Io)?)
    }

    async fn read(&self, path: &Path) -> Result<Vec<u8>> {
        let abs_path = self.absolute_path(path)?;
        Ok(fs::read(&abs_path).await.map_err(|e| Self::map_io_error(e, path))?)
    }

    async fn read_head(&self, path: &Path, bytes: usize) -> Result<Vec<u8>> {
        let abs_path = self.absolute_path(path)?;
        let file = fs::File::open(&abs_path).await.map_err(|e| Self::map_io_error(e, path))?;
        let mut buffer = Vec::with_capacity(bytes);
        file.take(bytes as u64).read_to_end(&mut buffer).await.map_err(ErrorKind::Io)?;
        Ok(buffer)
    }

    async fn write(&self, path: &Path, data: &[u8]) -> Result<()> {
        let abs_path = self.absolute_path(path)?;
        // Create parent directories if needed, to keep behaviour
        // consistent with S3-compatible storage.
        if let Some(parent) = abs_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| Self::map_io_error(e, path))?;
        }
        Ok(fs::write(&abs_path, data).await.map_err(|e| Self::map_io_error(e, path))?)
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        let abs_path = self.absolute_path(path)?;
        Ok(fs::remove_file(&abs_path).await.map_err(|e| Self::map_io_error(e, path))?)
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let from_path = self.absolute_path(from)?;
        let to_path = self.absolute_path(to)?;
        // Create parent directories for destination if needed
        if let Some(parent) = to_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| Self::map_io_error(e, to))?;
        }
        Ok(fs::rename(&from_path, &to_path).await.map_err(|e| Self::map_io_error(e, to))?)
    }

    async fn stat(&self, path: &Path) -> Result<FileInfo> {
        let abs_path = self.absolute_path(path)?;
        let metadata = fs::metadata(&abs_path).await.map_err(|e| Self::map_io_error(e, path))?;
        Self::metadata(path, metadata)
    }
}

#[cfg(test)]
mod tests {
    use crate::error::ErrorKind;
    use rawr_compress::Compression;

    use super::*;

    #[test]
    fn test_new_requires_absolute_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert!(LocalBackend::new("name", temp_dir.path()).is_ok());
        assert!(LocalBackend::new("name", "relative/path").is_err());
        assert!(LocalBackend::new("name", "./relative").is_err());
    }

    #[test]
    fn test_absolute_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
        let expected = temp_dir.path().join("Fandom/work.html.bz2");
        assert_eq!(backend.absolute_path(Path::new("Fandom/work.html.bz2")).unwrap(), expected);
        // Path traversal is prevented
        assert!(backend.absolute_path(Path::new("../etc/passwd")).is_err());
    }

    #[test]
    fn test_relative_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
        let abs = temp_dir.path().join("Fandom/work.html.bz2");
        assert_eq!(backend.relative_path(&abs).unwrap(), Path::new("Fandom/work.html.bz2"));
        // Path outside root fails
        let outside = PathBuf::from("/other/file.html");
        assert!(backend.relative_path(&outside).is_err());
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
        let data = b"Hello, world!";
        backend.write(Path::new("test.txt"), data).await.unwrap();
        let read_data = backend.read(Path::new("test.txt")).await.unwrap();
        assert_eq!(read_data, data);
    }

    #[tokio::test]
    async fn test_prefix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
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
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
        backend.write(Path::new("a/b/c/file.txt"), b"data").await.unwrap();
        assert!(backend.exists(Path::new("a/b/c/file.txt")).await.unwrap());
    }

    #[tokio::test]
    async fn test_exists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
        assert!(!backend.exists(Path::new("nonexistent.txt")).await.unwrap());
        backend.write(Path::new("exists.txt"), b"data").await.unwrap();
        assert!(backend.exists(Path::new("exists.txt")).await.unwrap());
    }

    #[tokio::test]
    async fn test_read_head() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
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
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
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
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
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
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
        backend.write(Path::new("file.txt"), b"data").await.unwrap();
        backend.rename(Path::new("file.txt"), Path::new("a/b/c/file.txt")).await.unwrap();
        assert!(backend.exists(Path::new("a/b/c/file.txt")).await.unwrap());
    }

    #[tokio::test]
    async fn test_stat() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
        let data = b"Hello, world!";
        backend.write(Path::new("file.txt"), data).await.unwrap();
        let info = backend.stat(Path::new("file.txt")).await.unwrap();
        assert_eq!(info.path, PathBuf::from("file.txt"));
        assert_eq!(info.size, data.len() as u64);
        assert_eq!(info.compression, Compression::None);
        assert!(info.file_hash.is_none());
    }

    #[tokio::test]
    async fn test_list_empty_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
        let files = backend.list(None).await.unwrap();
        assert_eq!(files.len(), 0);
    }

    #[tokio::test]
    async fn test_list_returns_all_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
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
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
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
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
        let files = backend.list(Some(Path::new("nonexistent/"))).await.unwrap();
        assert_eq!(files.len(), 0);
    }

    #[tokio::test]
    async fn test_path_security() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::new("name", temp_dir.path()).unwrap();
        // Attempts to escape the root should fail
        assert!(backend.read(Path::new("../etc/passwd")).await.is_err());
        assert!(backend.read(Path::new("etc/../../passwd")).await.is_err());
        assert!(backend.write(Path::new("../etc/passwd"), b"data").await.is_err());
        assert!(backend.delete(Path::new("../../file")).await.is_err());
    }
}
