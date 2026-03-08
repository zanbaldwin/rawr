//! Shared OpenDAL utilities for storage backends.

use crate::error::ErrorKind;
use crate::file::FileInfo;
use rawr_compress::Compression;
use std::path::{Path, PathBuf};
use time::UtcDateTime;

// BIG BIG TODO: Use strings instead of paths for this entire crate, and the
// rest of the workspace. See crate::path::ValidatedPath.

/// Map an [`opendal::Error`] to our [`ErrorKind`].
pub fn map_opendal_error(e: opendal::Error, path: &Path) -> ErrorKind {
    match e.kind() {
        opendal::ErrorKind::NotFound => ErrorKind::NotFound(path.display().to_string()),
        opendal::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied(path.display().to_string()),
        opendal::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists(path.display().to_string()),
        _ if e.is_temporary() => ErrorKind::Network(e.to_string()),
        _ => ErrorKind::BackendError(e.to_string()),
    }
}

/// Convert OpenDAL [`opendal::Metadata`] into a [`FileInfo`] for a given path.
pub fn metadata_to_file_info(backend_name: &str, path: PathBuf, meta: &opendal::Metadata) -> FileInfo {
    let size = meta.content_length();
    let modified = meta
        .last_modified()
        .and_then(|ts| UtcDateTime::from_unix_timestamp(ts.timestamp()).ok())
        .unwrap_or(UtcDateTime::UNIX_EPOCH);
    let compression = Compression::from_path(&path);
    // Safety: we'll be replace Path(Buf)s complete soon, this expect will disappear with it.
    FileInfo::new(backend_name, path, size, modified, compression).expect("File path should be valid")
}
