use crate::error::{ErrorKind as LibraryErrorKind, Result as LibraryResult};
use crate::scan::error::{ErrorKind, Result as ScanResult};
use exn::ResultExt;
use rawr_cache::{ExistenceResult, Repository};
use rawr_extract::extract;
use rawr_extract::models::Version;
use rawr_storage::BackendHandle;
use rawr_storage::file::{Discovered, FileInfo, Processed};

/// Indicates how a scan result was obtained.
///
/// Used to distinguish between cache hits and actual processing work,
/// which is useful for progress reporting and performance analysis.
pub enum ScanEffort {
    /// File was already in the cache with matching hash.
    Cached,
    /// File was in cache but hash changed; content was re-extracted.
    Recalculated,
    /// File was not in cache; content was freshly extracted.
    Processed,
}

pub struct Scan {
    pub file: FileInfo<Processed>,
    pub version: Version,
    pub effort: ScanEffort,
}

pub async fn scan_file(backend: &BackendHandle, cache: &Repository, file: FileInfo<Discovered>) -> LibraryResult<Scan> {
    scan_file_inner(backend, cache, file).await.or_raise(|| LibraryErrorKind::Scan)
}

pub(crate) async fn scan_file_inner(
    backend: &BackendHandle,
    cache: &Repository,
    file: FileInfo<Discovered>,
) -> ScanResult<Scan> {
    let existing = cache.get_by_target_path(backend.name(), &file.path).await.or_raise(|| ErrorKind::Cache)?;
    if let Some((cached_file, version)) = existing
        && file.size == cached_file.size
    {
        let effort = ScanEffort::Cached;
        return Ok(Scan { file: cached_file, version, effort });
    }
    // All that effort with Read/Write traits? Apparently pointless... Now the
    // entire file contents is going to be stored in the future's state machine.
    let bytes = backend.read(&file.path).await.or_raise(|| ErrorKind::Storage)?;
    let file = file.with_file_hash(blake3::hash(&bytes).to_string());
    let existing = cache.exists(backend.name(), &file.path, &file.file_hash).await.or_raise(|| ErrorKind::Cache)?;
    let effort = match existing {
        // If we get to this point with an ExactMatch (unlikely) it means that
        // the file hash was the same but the file size wasn't. Data integrity
        // is now in question: recalculate.
        ExistenceResult::ExactMatch(_, _) | ExistenceResult::HashMismatch(_, _) => {
            cache.delete_by_target_path(backend.name(), &file.path).await.or_raise(|| ErrorKind::Cache)?;
            tracing::info!(target = backend.name(), path = %file.path.display(), "Cached file has changed on disk; recalculating");
            ScanEffort::Recalculated
        },
        ExistenceResult::LocatedElsewhere(other, version) => {
            let file = file.with_content_hash(other.content_hash);
            cache.upsert(&file, &version).await.or_raise(|| ErrorKind::Cache)?;
            return Ok(Scan {
                file,
                version,
                effort: ScanEffort::Cached,
            });
        },
        ExistenceResult::NotFound => ScanEffort::Processed,
    };
    let content = file.compression.decompress(&bytes).or_raise(|| ErrorKind::Compression)?;
    let version = extract(&content).or_raise(|| ErrorKind::Extract)?;
    let file = file.with_content_hash(&version.hash);
    cache.upsert(&file, &version).await.or_raise(|| ErrorKind::Cache)?;
    Ok(Scan { file, version, effort })
}
