use crate::Context;
use crate::error::{ErrorKind as LibraryErrorKind, Result as LibraryResult};
use crate::organize::conflict::handle_conflict;
use crate::organize::error::{ErrorKind as OrganizeErrorKind, Result as OrganizeResult};
use crate::scan::error::ErrorKind as ScanErrorKind;
use crate::scan::{Scan, file::scan_file_inner};
use exn::ResultExt;
use rawr_cache::Repository;
use rawr_compress::Compression;
use rawr_storage::BackendHandle;
use rawr_storage::error::ErrorKind as StorageErrorKind;
use rawr_storage::file::{FileInfo, HashState};
use std::io::{self, Cursor};
use std::ops::Deref;
use std::path::PathBuf;

/// The outcome of (successfully) organizing a single file.
///
/// Each variant carries the relevant path — either the new location or the
/// path that was cleaned up. Consumers can pattern-match to decide whether
/// to log, report progress, or take further action.
pub enum Action {
    /// File was moved (and optionally re-compressed) to the correct path.
    Renamed(PathBuf),
    /// File was already at the correct path; no work performed.
    AlreadyCorrect(PathBuf),
    /// File no longer exists on disk (or a duplicate already existed in the
    /// location it was going to be moved to); its record was cleaned up.
    CleanedUp(PathBuf),
}

/// Moves a single file to its intended, template-derived location, handling
/// conflicts and compression conversion.
///
/// Looks up the file's [`Version`](rawr_extract::models::Version) in the
/// [`Repository`] cache, computes the correct path via the [`Context`]'s
/// [`PathGenerator`](crate::PathGenerator), and takes one of three actions:
///
/// - **[`Action::AlreadyCorrect`]** — the file is already where it belongs.
/// - **[`Action::Renamed`]** — the file was moved (re-compressed if needed).
/// - **[`Action::CleanedUp`]** — either:
///   - the file did not exist, and its cache entry was cleaned up, or
///   - a duplicate of that particular version already existed in the target
///     location, and the original was cleaned up.
///
/// When the target path is occupied by a file of a different version, conflict
/// resolution recursively relocates the occupant first.
///
/// # Errors
/// Returns [`Exn<LibraryErrorKind::Organize>`](LibraryErrorKind::Organize)
/// raised from an inner [`Exn<OrganizeErrorKind>`](OrganizeErrorKind).
pub async fn organize_file<S: HashState>(
    backend: &BackendHandle,
    cache: &Repository,
    ctx: &Context,
    file: FileInfo<S>,
) -> LibraryResult<Action> {
    organize_file_inner(backend, cache, ctx, file, vec![]).await.or_raise(|| LibraryErrorKind::Organize)
}

/// Inner implementation that carries a `depth` stack for cycle detection
/// during recursive conflict resolution.
pub(crate) async fn organize_file_inner<S: HashState>(
    backend: &BackendHandle,
    cache: &Repository,
    ctx: &Context,
    file: FileInfo<S>,
    depth: Vec<PathBuf>,
) -> OrganizeResult<Action> {
    if file.target != backend.name() {
        exn::bail!(OrganizeErrorKind::Storage);
    }

    if !backend.exists(&file.path).await.or_raise(|| OrganizeErrorKind::Storage)? {
        cache.delete_by_target_path(&file.target, &file.path).await.or_raise(|| OrganizeErrorKind::Cache)?;
        return Ok(Action::CleanedUp(file.path.clone()));
    }

    let file_path = file.path.clone();
    let (file, version) =
        match cache.get_by_target_path(&file.target, &file.path).await.or_raise(|| OrganizeErrorKind::Cache)? {
            Some(r) => r,
            // File not in cache, we need to scan it first to get the metadata for
            // path generation. This is NOT the intended use-case (organizing files
            // not already in cache), but the function is public, so...
            None => match scan_file_inner(backend, cache, file).await {
                // We scanned the file and now it's cached.
                Ok(Scan { file, version, .. }) => (file, version),
                // The file doesn't exist in the cache and, when we tried to perform a scan, it wasn't valid.
                Err(e) if matches!(e.deref(), ScanErrorKind::Extract) => {
                    backend.delete(&file_path).await.or_raise(|| OrganizeErrorKind::Storage)?;
                    return Ok(Action::CleanedUp(file_path));
                },
                // An operational error occured during scanning.
                Err(e) => return Err(e).or_raise(|| OrganizeErrorKind::Scan),
            },
        };

    let compression_source = file.compression;
    let compression_target = ctx.compression.unwrap_or(compression_source);

    let correct_location =
        ctx.template.generate_with_ext(version, "html", compression_target).or_raise(|| OrganizeErrorKind::Template)?;
    if file.path == correct_location {
        return Ok(Action::AlreadyCorrect(file.path.clone()));
    }

    if let Some(existing) = match backend.stat(&correct_location).await {
        Ok(f) => Some(f),
        Err(e) if matches!(e.deref(), StorageErrorKind::NotFound(_)) => None,
        Err(e) => Err(e).or_raise(|| OrganizeErrorKind::Storage)?,
    } {
        match handle_conflict(backend, cache, ctx, &file, existing, depth).await {
            Ok(Some(r)) => return Ok(r),
            Ok(None) => (), // continue...
            Err(e) => return Err(e),
        }
    }
    // Target location is now free. If there was a cache entry at the target location
    // it isn't there now, delete old entry. Silently ignore errors if it couldn't
    // be deleted, it's a dangling record anyway.
    _ = cache.delete_by_target_path(&file.target, &file.path).await;

    if compression_source == compression_target {
        // The file is already compressed using the correct format, a simple rename will do.
        backend.rename(&file.path, &correct_location).await.or_raise(|| OrganizeErrorKind::Storage)?;
    } else {
        let converted = convert(
            &backend.read(&file.path).await.or_raise(|| OrganizeErrorKind::Storage)?,
            compression_source,
            compression_target,
        )
        .or_raise(|| OrganizeErrorKind::Compression)?;
        backend.write(&correct_location, &converted).await.or_raise(|| OrganizeErrorKind::Storage)?;
        backend.delete(&file.path).await.or_raise(|| OrganizeErrorKind::Storage)?;
    }

    // Update the cache with the new location, but silently ignore errors since
    // it can be cleaned up on the next library scan operation.
    _ = cache.update_target_path(&file.target, &file.path, &correct_location).await;
    Ok(Action::Renamed(correct_location))
}

/// Convert from one compression format to another
fn convert(data: &[u8], source: Compression, target: Compression) -> OrganizeResult<Vec<u8>> {
    let reader = Cursor::new(data);
    let mut decompressor = source.wrap_reader(reader).or_raise(|| OrganizeErrorKind::Compression)?;
    let mut writer = Cursor::new(Vec::new());
    let mut compressor = target.wrap_writer(&mut writer).or_raise(|| OrganizeErrorKind::Compression)?;
    io::copy(&mut decompressor, &mut compressor).or_raise(|| OrganizeErrorKind::Compression)?;
    drop(compressor);
    Ok(writer.into_inner())
}
