use std::sync::{Arc, Mutex};

use crate::error::{ErrorKind as LibraryErrorKind, Result as LibraryResult};
use crate::scan::error::{ErrorKind, Result as ScanResult};
use exn::ResultExt;
use futures::io::{copy as async_copy, sink as async_devnull};
use rawr_asyncutils::InspectReader;
use rawr_cache::{ExistenceResult, Repository};
use rawr_extract::models::Version;
use rawr_extract::{ESTIMATED_HEADER_SIZE_BYTES, Extractor};
use rawr_storage::BackendHandle;
use rawr_storage::file::{FileInfo, HashState, Processed};
use time::UtcDateTime;

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
    // Data calculated as the file streams through the decompression pipeline.
    let file_hasher = Arc::new(Mutex::new(blake3::Hasher::new()));
    let file_size = Arc::new(Mutex::new(0u64));
    let mut content_hasher = blake3::Hasher::new();
    let mut crc_hasher = crc32fast::Hasher::new();
    let mut content_length = 0u64;

    let file_reader = InspectReader::new(backend.reader(&file.path).await.or_raise(|| ErrorKind::Storage)?, {
        let file_hasher = file_hasher.clone();
        let file_size = file_size.clone();
        move |bytes: &[u8]| {
            *file_size.lock().unwrap() += bytes.len() as u64;
            file_hasher.lock().unwrap().update(bytes);
        }
    });
    let mut peekable = file.compression.async_peekable_reader(file_reader).or_raise(|| ErrorKind::Compression)?;

    let head = peekable.peek(ESTIMATED_HEADER_SIZE_BYTES).await.or_raise(|| ErrorKind::Compression)?;
    let metadata = Extractor::from_long_html(head).metadata().or_raise(|| ErrorKind::Extract)?;

    let mut content_reader = InspectReader::new(peekable.into_reader(), |bytes: &[u8]| {
        content_hasher.update(bytes);
        crc_hasher.update(bytes);
        content_length += bytes.len() as u64;
    });
    async_copy(&mut content_reader, &mut async_devnull()).await.or_raise(|| ErrorKind::Io)?;

    let file = file.with_file_hash(file_hasher.lock().unwrap().finalize().to_string());
    let existing = cache.exists(backend.name(), &file.path, &file.file_hash).await.or_raise(|| ErrorKind::Cache)?;
    // Unfortunately now the "effort" is a little misleading because we calculate
    // both the file hash and the content hash at the same time as we're streaming
    // it through the decompression pipeline.
    // Effort is less about the effort it took, and more just information about
    // what already existed in the cache.
    let effort = match existing {
        // If we get to this point with an ExactMatch (unlikely) it means that
        // the file hash was the same but the file size wasn't. Data integrity
        // is now in question: recalculate.
        ExistenceResult::ExactMatch(_, _) | ExistenceResult::HashMismatch(_, _) => {
            cache.delete_by_target_path(backend.name(), &file.path).await.or_raise(|| ErrorKind::Cache)?;
            tracing::info!(target = backend.name(), path = %file.path, "Cached file has changed on disk; recalculating");
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
    // TODO: table rows are missing file size and discovered_at (both default to zero).
    let version = Version {
        hash: content_hasher.finalize().to_string(),
        crc32: crc_hasher.finalize(),
        length: content_length,
        metadata,
        extracted_at: UtcDateTime::now(),
    };
    let file = file.with_content_hash(&version.hash);
    cache.upsert(&file, &version).await.or_raise(|| ErrorKind::Cache)?;
    Ok(Scan { file, version, effort })
}
