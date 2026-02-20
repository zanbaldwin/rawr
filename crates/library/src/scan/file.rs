use crate::error::{ErrorKind as LibraryErrorKind, Result as LibraryResult};
use crate::scan::error::{ErrorKind, Result as ScanResult};
use exn::ResultExt;
use rawr_cache::{ExistenceResult, Repository};
use rawr_extract::extract;
use rawr_extract::models::Version;
use rawr_storage::BackendHandle;
use rawr_storage::file::{FileInfo, HashState, Processed};

/// Indicates how much work was required to produce a [`Scan`] result.
///
/// Distinguishes between cache hits and actual extraction work, which is
/// useful for progress reporting and performance analysis.
pub enum ScanEffort {
    /// The file's path and size matched a cache entry — no I/O or extraction
    /// was performed. Also used when the file hash matches a record at a
    /// different path (content deduplication).
    Cached,
    /// The file existed in cache but its hash changed on disk, so the content
    /// was decompressed and re-extracted.
    Recalculated,
    /// No cache entry existed for this file; content was freshly decompressed
    /// and extracted.
    Processed,
}

/// The result of scanning a single file.
///
/// Contains the fully-hashed [`FileInfo`] (with both file and content hashes
/// computed), the extracted [`Version`] metadata, and a [`ScanEffort`]
/// indicating whether the result came from cache or fresh extraction.
pub struct Scan {
    pub file: FileInfo<Processed>,
    pub version: Version,
    pub effort: ScanEffort,
}

/// Scans a single file, extracting its metadata or returning a cached result.
///
/// The file goes through a multi-layered cache lookup before falling back to
/// full extraction:
///
/// 1. **Path + size match** — if the cache has an entry at the same path with
///    the same file size, the cached result is returned immediately (no I/O).
/// 2. **Hash match at different path** — if the file's BLAKE3 hash matches a
///    record elsewhere, the content hash is reused (content deduplication).
/// 3. **Hash mismatch** — if the path exists in cache but hashes differ, the
///    old entry is deleted and the file is re-extracted.
/// 4. **Not found** — the file is decompressed and fully extracted.
///
/// The input [`FileInfo`] can be in any [`HashState`]; existing hashes are
/// stripped and recomputed from the file contents.
pub async fn scan_file<S: HashState>(
    backend: &BackendHandle,
    cache: &Repository,
    file: FileInfo<S>,
) -> LibraryResult<Scan> {
    scan_file_inner(backend, cache, file).await.or_raise(|| LibraryErrorKind::Scan)
}

pub(crate) async fn scan_file_inner<S: HashState>(
    backend: &BackendHandle,
    cache: &Repository,
    file: FileInfo<S>,
) -> ScanResult<Scan> {
    let file = file.strip_hashes();
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
