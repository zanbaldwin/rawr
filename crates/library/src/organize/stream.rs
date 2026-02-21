use crate::error::{ErrorKind as LibraryErrorKind, Result as LibraryResult};
use crate::organize::error::{ErrorKind as OrganizeErrorKind, Result as OrganizeResult};
use crate::organize::file::{Action, organize_file_inner};
use crate::{Context, MAX_PROCESS_CONCURRENCY};
use async_stream::stream;
use exn::ResultExt;
use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt};
use rawr_cache::Repository;
use rawr_storage::BackendHandle;

/// Progress events emitted by [`organize`] as it works through a storage
/// backend's cached files.
///
/// Events follow a strict ordering:
/// 1. [`Started`](Self::Started) — exactly once.
/// 2. [`DiscoveryComplete`](Self::DiscoveryComplete) — exactly once, with the
///    total file count.
/// 3. [`Organized`](Self::Organized) — zero or more times, one per file.
/// 4. [`Complete`](Self::Complete) — exactly once, signalling the stream is
///    finished.
///
/// An error may terminate the stream early, in which case [`Complete`](Self::Complete)
/// is never emitted.
pub enum OrganizeEvent {
    /// Organizing has begun; emitted exactly once before any other event.
    Started,
    /// All cache entries have been discovered; the total count is now known.
    DiscoveryComplete(u64),
    /// A file has been organized.
    Organized(Action),
    /// All discovered cache entries have been organized; the stream is finished.
    Complete,
}

/// Streams [`OrganizeEvent`]s for every cached file in `backend`, relocating
/// each one to its template-derived path according to `ctx`.
///
/// Files are discovered by querying the [`Repository`] cache for all entries
/// belonging to the given [`BackendHandle`], then organized concurrently up to
/// `MAX_PROCESS_CONCURRENCY` (100) at a time. Additional files are promoted as
/// in-flight operations complete.
///
/// The stream yields events in the order documented on [`OrganizeEvent`].
/// Individual file failures are surfaced as `Err` items without terminating
/// the stream — only a cache discovery failure is fatal.
pub fn organize<'a>(
    backend: &'a BackendHandle,
    cache: &'a Repository,
    ctx: &'a Context,
) -> impl Stream<Item = LibraryResult<OrganizeEvent>> + 'a {
    // `rustfmt` does not format macro-specific syntax such as
    // `for await` even using the parentheses trick.
    stream! {
        for await event in organize_inner(backend, cache, ctx) {
            yield event.or_raise(|| LibraryErrorKind::Organize);
        }
    }
}

fn organize_inner<'a>(
    backend: &'a BackendHandle,
    cache: &'a Repository,
    ctx: &'a Context,
) -> impl Stream<Item = OrganizeResult<OrganizeEvent>> + 'a {
    // `rustfmt` does not format macros that use braces. Wrap in parentheses!
    stream!({
        yield Ok(OrganizeEvent::Started);

        let files = match cache.list_files_for_target(backend.name()).await.or_raise(|| OrganizeErrorKind::Cache) {
            Ok(f) => f,
            Err(e) => {
                yield Err(e);
                return;
            },
        };
        // Infallible: a usize (either 32- or 64-bit) will always fit in a u64.
        yield Ok(OrganizeEvent::DiscoveryComplete(u64::try_from(files.len()).unwrap_or(0)));

        let mut futures: Vec<_> =
            files.into_iter().map(|(file, _version)| organize_file_inner(backend, cache, ctx, file, vec![])).collect();
        let mut processing = FuturesUnordered::new();
        processing.extend(futures.drain(..MAX_PROCESS_CONCURRENCY.min(futures.len())));
        while let Some(result) = processing.next().await {
            yield result.map(OrganizeEvent::Organized);
            // Pop-n-push, but FIFO instead of LIFO.
            if !futures.is_empty() {
                processing.push(futures.remove(0));
            }
        }

        yield Ok(OrganizeEvent::Complete);
    })
}
