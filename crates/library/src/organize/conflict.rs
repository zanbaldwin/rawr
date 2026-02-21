use crate::Context;
use crate::organize::error::{ErrorKind as OrganizeErrorKind, Result as OrganizeResult};
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
use time::UtcDateTime;

/// Maximum recursive relocations before bailing with [`OrganizeErrorKind::Conflict`].
const MAX_CONFLICT_DEPTH: usize = 5;

/// Resolves a path collision between an `incoming` file and an `existing` file
/// that already occupies the target location.
///
/// Returns `Ok(Some(action))` when the conflict is fully resolved and the
/// caller should use that [`Action`] as the final result. Returns `Ok(None)`
/// when the existing file has been moved out of the way and the caller should
/// proceed with its own rename.
///
/// **Resolution strategy:**
/// 1. If the existing file isn't cached, scan it first. If scanning fails
///    because the file is invalid ([`ScanErrorKind::Extract`]), delete it and
///    yield the slot.
/// 2. If both files share the same content hash, they're duplicates — delete
///    the incoming file and return [`Action::CleanedUp`].
/// 3. Otherwise, recursively [`organize_file_inner`] the existing file to
///    relocate it, bounded by `depth` to prevent infinite chains.
/// 4. If the existing file is already at *its* correct location (i.e. a true
///    collision), the incoming file is written to the trash backend (if
///    configured) and an [`OrganizeErrorKind::Conflict`] error is raised.
pub(crate) async fn handle_conflict<S: HashState>(
    backend: &BackendHandle,
    cache: &Repository,
    ctx: &Context,
    incoming: &FileInfo<Processed>,
    existing: FileInfo<S>,
    mut depth: Vec<PathBuf>,
) -> OrganizeResult<Option<Action>> {
    let existing_path = existing.path.clone();
    let existing =
        match cache.get_by_target_path(&incoming.target, &existing.path).await.or_raise(|| OrganizeErrorKind::Cache)? {
            Some((cached, _)) => cached,
            None => match scan_file_inner(backend, cache, existing).await {
                // We scanned the target file and now it's cached, ready for conflict resolution.
                Ok(Scan { file, .. }) => file,
                // The target file doesn't exist in the cache, and when we tried to perform a scan, it wasn't valid.
                Err(e) if matches!(e.deref(), ScanErrorKind::Extract) => {
                    backend.delete(&existing_path).await.or_raise(|| OrganizeErrorKind::Storage)?;
                    return Ok(None);
                },
                // An operational error occured during scanning.
                Err(e) => return Err(e).or_raise(|| OrganizeErrorKind::Scan),
            },
        };

    if incoming.content_hash == existing.content_hash {
        // The existing file is already the correct version. Since we calcuate target
        // locations from immutable metadata (and detect compression using file extension),
        // it's safe to assume that the existing file is exactly where it needs to be.
        // Delete the incoming file.
        backend.delete(&incoming.path).await.or_raise(|| OrganizeErrorKind::Storage)?;
        return Ok(Some(Action::CleanedUp(incoming.path.clone())));
    }
    // Content hashes are different. Existing file needs relocation.
    if depth.len() > MAX_CONFLICT_DEPTH || depth.contains(&existing.path) {
        exn::bail!(OrganizeErrorKind::Conflict);
    }
    depth.push(existing.path.clone());
    // Arc-pointer, just clone the damn thing.
    let trash = ctx.trash.clone();
    // Pin that sucker! Otherwise you have some weird async recursion error
    // that is so complicated it makes your brain explode...
    match Box::pin(organize_file_inner(backend, cache, ctx, existing, depth)).await {
        Ok(Action::AlreadyCorrect(_)) => {
            if let Some(trash) = trash {
                // Which one do we trash?
                let now = UtcDateTime::now();
                let trash_name = PathBuf::from(format!(
                    "{}-{}.html{}",
                    incoming.file_hash,
                    now.unix_timestamp(),
                    incoming.compression.extension()
                ));
                let contents = backend.read(&incoming.path).await.or_raise(|| OrganizeErrorKind::Storage)?;
                trash.write(&trash_name, &contents).await.or_raise(|| OrganizeErrorKind::Storage)?;
                backend.delete(&incoming.path).await.or_raise(|| OrganizeErrorKind::Storage)?;
            }
            exn::bail!(OrganizeErrorKind::Conflict);
        },
        Ok(Action::Renamed(_)) | Ok(Action::CleanedUp(_)) => Ok(None),
        Err(e) => Err(e),
    }
}
