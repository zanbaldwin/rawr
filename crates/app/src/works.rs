//! Finding works in the library and reading their HTML.
//!
//! Shared by every front-end that takes a work from the user: export,
//! narration, and the web server.

use crate::error::{ErrorKind, Result};
use exn::ResultExt;
use rawr_cache::Repository;
use rawr_extract::models::Version;
use rawr_storage::BackendHandle;
use rawr_storage::file::{FileInfo, Processed};
use std::str::FromStr;

/// A file record from the cache.
pub type File = FileInfo<Processed>;

/// A user-supplied reference to a work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkRef {
    /// Just a work ID: the best version.
    BestWork(u64),
    /// Work ID with CRC32 hash: a specific version.
    /// The u32 is parsed from 8 hex digits (e.g., `12345@37bc3355`).
    /// Matched against `Version.crc32` during resolution.
    WorkVersion(u64, u32),
    /// A file path in the import target.
    FilePath(String),
}

impl FromStr for WorkRef {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Ok(id) = s.parse::<u64>() {
            return Ok(WorkRef::BestWork(id));
        }
        if let Some((id_str, hex)) = s.split_once('@')
            && let Ok(id) = id_str.parse::<u64>()
            && let Ok(crc) = u32::from_str_radix(hex, 16)
        {
            return Ok(WorkRef::WorkVersion(id, crc));
        }
        Ok(WorkRef::FilePath(s.to_string()))
    }
}

/// Find the version a reference points at, with its file records.
///
/// `target` is the name of the import target, for file paths.
///
/// # Errors
///
/// - [`WorkNotFound`](ErrorKind::WorkNotFound), [`VersionNotFound`](ErrorKind::VersionNotFound)
///   or [`FileNotFound`](ErrorKind::FileNotFound) if nothing matches
/// - [`Cache`](ErrorKind::Cache) if the lookup fails
pub async fn resolve(cache: &Repository, target: &str, work: &WorkRef) -> Result<(Version, Vec<File>)> {
    match work {
        WorkRef::BestWork(id) => cache
            .get_best_for_work_id(*id)
            .await
            .or_raise(|| ErrorKind::Cache)?
            .ok_or_else(|| exn::Exn::new(ErrorKind::WorkNotFound(*id))),
        WorkRef::WorkVersion(id, crc) => cache
            .get_by_work_id(*id)
            .await
            .or_raise(|| ErrorKind::Cache)?
            .into_iter()
            .find(|(version, _)| version.crc32 == *crc)
            .ok_or_else(|| exn::Exn::new(ErrorKind::VersionNotFound { work: *id, crc32: *crc })),
        WorkRef::FilePath(path) => cache
            .get_by_target_path(target, path.as_str())
            .await
            .or_raise(|| ErrorKind::Cache)?
            .map(|(file, version)| (version, vec![file]))
            .ok_or_else(|| {
                exn::Exn::new(ErrorKind::FileNotFound {
                    target: target.to_string(),
                    path: path.clone(),
                })
            }),
    }
}

/// Read and decompress the first of a version's files that exists in the backend.
///
/// Returns `None` if none of the files exists.
///
/// # Errors
///
/// [`Read`](ErrorKind::Read) if the file exists but cannot be read or decompressed.
pub async fn read_html(backend: &BackendHandle, files: Vec<File>) -> Result<Option<(File, Vec<u8>)>> {
    for file in files {
        if backend.exists(&file.path).await.is_ok_and(|exists| exists) {
            let compressed = backend.read(&file.path).await.or_raise(|| ErrorKind::Read(file.path.to_string()))?;
            let html = file.compression.decompress(&compressed).or_raise(|| ErrorKind::Read(file.path.to_string()))?;
            return Ok(Some((file, html)));
        }
    }
    Ok(None)
}
