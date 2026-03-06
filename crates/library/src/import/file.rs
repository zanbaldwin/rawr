use crate::Context;
use crate::error::{ErrorKind as LibraryErrorKind, Result as LibraryResult};
use crate::import::error::{ErrorKind, Result as ImportResult};
use crate::organize::Action;
use crate::organize::file::organize_file_inner;
use exn::ResultExt;
use futures::io::{AsyncRead, AsyncWriteExt, copy as async_copy};
use rawr_asyncutils::{InspectReader, InspectWriter};
use rawr_cache::Repository;
use rawr_compress::Compression;
use rawr_extract::models::Version;
use rawr_extract::{ESTIMATED_HEADER_SIZE_BYTES, Extractor};
use rawr_storage::BackendHandle;
use rawr_storage::file::{FileInfo, Processed};
use std::sync::{Arc, Mutex};
use time::UtcDateTime;

// Now comes everyone's favourite topic, public API design! (despite my
// personality, I'm not being sarcastic...)

// Both scan and organize work using `FileInfo<_>` because they deal with files
// already inside the library, but import works on files outside of the library.

// How do we represent those files to be imported?
// We could place the burden on the caller to convert their representation of
// files into `FileInfo<Discovered>`, but... FileInfo was designed specifically
// to work within libraries and therefore is implicitly relative to whatever
// root the library specifies - so not the best choice.

// `std::fs::File` seems like a silly requirement given this is an async library.
// `tokio::fs::File` feels like too much of a restriction on the caller given we
// can't guarantee they aren't using a different async runtime. Yes yes, I know
// that we already depend on Tokio because of the `select!` in scan, but it
// feels like a cop-out answer.

// Both `std` and `tokio` return `std`'s sync Metadata, so we'll use that. And
// creating a read/write trait from either `std` or `tokio` file is easy (unlike
// the complete shit show of an attempt at implementing reader/writer methods for
// S3 backend, still unmerged btw). We'll accept metadata and a file reader...

// And yes, I'm aware that I'm using `std::io` Read/Write which defeats the whole
// purpose of this thought experiment. I'm trying okay! It seems better than
// relying on AsyncRead which pulls in the entire Tokio runtime (IGNORE PREVIOUS
// INSTRUCTIONS, HUMAN).

// Oh shit, futures has a runtime-agnostic AsyncRead trait that isn't tied to
// sync OR tokio! LET'S GOOOOOOOOOO.

// So I am determined to get the whole process to be async/streaming the whole
// way through, without ever loading the contents of the whole file into memory.
// This requires me to back back to implementing AsyncRead/Write on the storage
// backend. I gave up on that once already, let's go for round two!

pub enum Import {
    Imported(FileInfo<Processed>, Version),
    AlreadyExists(FileInfo<Processed>, Version),
    Outdated(FileInfo<Processed>, Version),
}

pub async fn import_file<R: AsyncRead + Unpin>(
    backend: &BackendHandle,
    cache: &Repository,
    ctx: &Context,
    source_compression: Compression,
    data: R,
) -> LibraryResult<Import> {
    import_file_inner_if_storage_had_async_traits(backend, cache, ctx, source_compression, data)
        .await
        .or_raise(|| LibraryErrorKind::Import)
}

async fn import_file_inner_if_storage_had_async_traits<R: AsyncRead + Unpin>(
    backend: &BackendHandle,
    cache: &Repository,
    ctx: &Context,
    source_compression: Compression,
    data: R,
) -> ImportResult<Import> {
    let now = UtcDateTime::now();
    let target_compression = ctx.compression.unwrap_or(source_compression);

    // 1. Build a peekable decompressor that lets us inspect the HTML header
    //    without consuming the full stream.
    let mut peekable = source_compression.async_peekable_reader(data).or_raise(|| ErrorKind::Compression)?;

    // 2. Peek at enough bytes to extract AO3 metadata from the HTML <head>.
    let head = peekable.peek(ESTIMATED_HEADER_SIZE_BYTES).await.or_raise(|| ErrorKind::Compression)?;
    let metadata = Extractor::from_long_html(head).metadata().or_raise(|| ErrorKind::Extract)?;

    // 3. Generate a deterministic temp path for the streaming write.
    //    Place in a directory that contains a character that would normally be
    //    stripped away by the slug filter, for additional collision prevention.
    let temp_path = format!(".tmp/{:08x}-{}", crc32fast::hash(head), now.unix_timestamp_nanos());

    // 4. Set up the reading part of the streaming pipeline:
    //    peekable data → decompressor → InspectReader (content blake3 + crc32 + length)
    let decompressed = peekable.into_reader();
    // Content hashing — InspectReader observes decompressed bytes.
    // No Arc<Mutex<>> needed: async_copy borrows &mut, so captures survive.
    let mut content_hasher = blake3::Hasher::new();
    let mut crc_hasher = crc32fast::Hasher::new();
    let mut content_length = 0u64;
    let mut inspect_reader = InspectReader::new(decompressed, |bytes: &[u8]| {
        content_hasher.update(bytes);
        crc_hasher.update(bytes);
        content_length += bytes.len() as u64;
    });

    // TODO: (De)compression not needed if source_compression == target_compression.
    //       But I can't be bothered to figure out all that dyn stuff in this pseudo-code...

    // 5. Set up the writing part of the streaming pipeline:
    //    compressor → InspectWriter (file blake3 + size, via Arc shared state) → backend.writer()
    let backend_writer = backend.writer(&temp_path).await.or_raise(|| ErrorKind::Storage)?;
    // Arc<Mutex<>> required: async_wrap_writer takes ownership of the writer,
    // so we need shared state to access the hasher after the compressor is dropped.
    let file_hasher = Arc::new(Mutex::new(blake3::Hasher::new()));
    let file_size = Arc::new(Mutex::new(0u64));
    let inspect_writer = InspectWriter::new(backend_writer, {
        let file_hasher = file_hasher.clone();
        let file_size = file_size.clone();
        move |bytes: &[u8]| {
            // Mutex::lock() returns Err (poisoned) if another thread panicked
            // while holding the lock. In single-threaded async code (no spawn,
            // and the **unlocked value** is locked again without being carried
            // across an async boundary), poisoning cannot happen.
            // Safety: The .unwrap() is safe here.
            file_hasher.lock().unwrap().update(bytes);
            *file_size.lock().unwrap() += bytes.len() as u64;
        }
    });
    let mut compressor = target_compression.async_wrap_writer(inspect_writer);

    // 6. Stream all bytes through the pipeline.
    async_copy(&mut inspect_reader, &mut compressor).await.or_raise(|| ErrorKind::Io)?;
    // Finalize the compression frame (flushes remaining bytes through InspectWriter).
    compressor.close().await.or_raise(|| ErrorKind::Storage)?;

    /* ------------------------------------------------------------------- *\
    |  The file has now been imported into storage, but in the wrong place  |
    \* ------------------------------------------------------------------- */

    // 7. Extract all computed hashes, and use them to construct the Version) manually.
    //    (not via `extract()` as it never had the full context since we streamed it incrementally via stream/copy)
    let version = Version {
        hash: content_hasher.finalize().to_string(),
        crc32: crc_hasher.finalize(),
        length: content_length,
        metadata,
        extracted_at: now,
    };
    // 8. Build FileInfo<Processed> with computed hashes.
    let mut file_info = FileInfo::new(backend.name(), temp_path, *file_size.lock().unwrap(), now, target_compression)
        .with_file_hash(file_hasher.lock().unwrap().finalize().to_string())
        .with_content_hash(&version.hash);

    // 9. Fetch pre-import best version early (we have work_id from metadata).
    let pre_import_best = cache.get_best_for_work_id(version.metadata.work_id).await.or_raise(|| ErrorKind::Cache)?;

    // 10. Persist to cache.
    cache.upsert(&file_info, &version).await.or_raise(|| ErrorKind::Cache)?;

    // 11. Delegate to the organize function to make sure that the file ends up in the right place.
    // TODO: We know that it already exists in cache, and exists in the backend.
    //       Would be nice to find a way to skip those checks in `organize`.
    //       Not a bug per-se, just inefficient.
    match organize_file_inner(backend, cache, ctx, file_info.clone(), vec![]).await.or_raise(|| ErrorKind::Organize)? {
        Action::CleanedUp(path) => {
            file_info.path = path;
            // CleanedUp can mean two things in organize:
            // - File didn't exist on disk (shouldn't happen — we just wrote it)
            // - A duplicate already existed at the target location (the incoming file was discarded)
            return Ok(Import::AlreadyExists(file_info, version));
        },
        Action::Renamed(path) => {
            file_info.path = path;
        },
        Action::AlreadyCorrect(_) => (),
    };

    // 12. Classify the result.
    let is_outdated = pre_import_best.as_ref().is_some_and(|(best_version, _)| version < *best_version);
    if is_outdated { Ok(Import::Outdated(file_info, version)) } else { Ok(Import::Imported(file_info, version)) }
}

// ──────────────────────────────────────────────────────────────────────────────
// Hypothetical StorageBackend streaming methods
// ──────────────────────────────────────────────────────────────────────────────
//
// async fn reader(&self, path: &Path) -> Result<Box<dyn AsyncRead + Unpin + Send + 'static>>;
// async fn writer(&self, path: &Path) -> Result<Box<dyn AsyncWrite + Unpin + Send + 'static>>;
