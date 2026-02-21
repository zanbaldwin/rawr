use crate::Context;
use crate::error::{ErrorKind as LibraryErrorKind, Result as LibraryResult};
use crate::import::error::Result as ImportResult;
use exn::ResultExt;
use futures::AsyncRead;
use rawr_cache::Repository;
use rawr_extract::models::Version;
use rawr_storage::BackendHandle;
use rawr_storage::file::{FileInfo, Processed};
use std::fs::Metadata;

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

pub enum Import {
    Imported(FileInfo<Processed>, Version),
    AlreadyExists(FileInfo<Processed>, Version),
    Outdated(FileInfo<Processed>, Version),
}

pub async fn import_file<W: AsyncRead>(
    backend: &BackendHandle,
    cache: &Repository,
    ctx: &Context,
    file: Metadata,
    data: W,
) -> LibraryResult<Import> {
    import_file_inner(backend, cache, ctx, file, data).await.or_raise(|| LibraryErrorKind::Import)
}

async fn import_file_inner<W: AsyncRead>(
    backend: &BackendHandle,
    cache: &Repository,
    ctx: &Context,
    file: Metadata,
    data: W,
) -> ImportResult<Import> {
    // TODO... Do we implement almost everything again here, or shall we dump
    //         the import into a random file in the library and delegate to
    //         organize_file_inner() to cleanup our mess?
    todo!()
}
