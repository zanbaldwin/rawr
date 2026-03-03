use crate::Context;
use crate::error::{ErrorKind as LibraryErrorKind, Result as LibraryResult};
use crate::organize::{Action, file::organize_file_inner};
use crate::scan::Scan;
use crate::scan::error::ErrorKind as ScanErrorKind;
use crate::scan::file::scan_file_inner;
use exn::ResultExt;
use rawr_cache::Repository;
use rawr_storage::BackendHandle;
use rawr_storage::file::{FileInfo, HashState, Processed};
use std::ops::Deref;
use std::path::PathBuf;

/// Maximum recursive relocations before bailing.
const MAX_CONFLICT_DEPTH: usize = 5;

pub(crate) enum ConflictResolution {
    /// The incoming file is not needed, it can be discarded/deleted.
    DiscardIncoming,
    /// The existing file is not needed, it can be discarded/deleted.
    DiscardExisting,
    /// The existing file has been moved out of the way, incoming is free to be placed.
    TargetNowFree,
    /// The conflict between incoming and existing files could not be resolved, but the existing file is better.
    TrashIncoming,
    /// The conflict between incoming and existing files could not be resolved, but the incoming file is better.
    TrashExisting,
}

/// Resolves a path collision between an `incoming` file and an `existing` file
/// that already occupies the target location.
///
/// Returns `Ok(Some(_))` when the conflict is fully resolved and the
/// caller should use that to determine how to implement the determined
/// resolution. Returns `Ok(None)` when the conflict cannot be resolved.
///
/// **Resolution strategy:**
/// 1. If the existing file isn't cached, scan it first. If scanning fails
///    because the file is invalid ([`ScanErrorKind::Extract`]), indicate that
///    the existing file can be deleted to yield the slot.
/// 2. If both files share the same content hash, they're duplicates — indicate
///    the incoming file can be deleted.
/// 3. Otherwise, recursively [`organize_file_inner`] the existing file to
///    relocate it, bounded by `depth` to prevent infinite chains.
/// 4. If the existing file is already at *its* correct location (i.e. a true
///    collision), determine if either of the files is a "better version" and
///    the other can be trashed.
/// 5. Otherwise, return that the collision could not be resolved.
pub(crate) async fn handle_conflict<S: HashState>(
    backend: &BackendHandle,
    cache: &Repository,
    ctx: &Context,
    incoming: &FileInfo<Processed>,
    existing: &FileInfo<S>,
    mut depth: Vec<PathBuf>,
) -> LibraryResult<Option<ConflictResolution>> {
    let (existing, _existing_version) = match cache
        .get_by_target_path(&existing.target, &existing.path)
        .await
        .or_raise(|| LibraryErrorKind::Conflict)?
    {
        Some((file, version)) => (file, version),
        None => match scan_file_inner(backend, cache, existing.clone()).await {
            // We scanned the target file and now it's cached, ready for conflict resolution.
            Ok(Scan { file, version, .. }) => (file, version),
            // The target file doesn't exist in the cache, and when we tried to perform a scan, it wasn't valid.
            Err(e) if matches!(e.deref(), ScanErrorKind::Extract) => {
                return Ok(Some(ConflictResolution::DiscardExisting));
            },
            // An operational error occured during scanning.
            Err(e) => return Err(e).or_raise(|| LibraryErrorKind::Conflict),
        },
    };
    if incoming.content_hash == existing.content_hash {
        // The existing file is already the correct version. Since we calcuate target
        // locations from immutable metadata (and detect compression using file extension),
        // it's safe to assume that the existing file is exactly where it needs to be.
        // This is the step where we should check which is already in the preferred
        // compression format (saving us a step), but it's easier to assume that if the
        // paths conflict, then so does the compression extension.
        return Ok(Some(ConflictResolution::DiscardIncoming));
    }
    // Content hashes are different. Existing file needs relocation.
    if depth.len() > MAX_CONFLICT_DEPTH || depth.contains(&existing.path) {
        exn::bail!(LibraryErrorKind::Conflict);
    }
    depth.push(existing.path.clone());
    // Pin that sucker! Otherwise you have some weird async recursion error
    // that is so complicated it makes your brain explode...
    match Box::pin(organize_file_inner(backend, cache, ctx, existing, depth)).await {
        // No conflict resolution is possible, return None.
        Ok(Action::AlreadyCorrect(_)) => {
            // TODO: Calculate is either of the files can be trashed.
            // Otherwise...
            Ok(None)
        },
        Ok(Action::Renamed(_)) | Ok(Action::CleanedUp(_)) => Ok(Some(ConflictResolution::TargetNowFree)),
        Err(e) => Err(e).or_raise(|| LibraryErrorKind::Conflict),
    }
}
