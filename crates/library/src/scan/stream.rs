use crate::error::{ErrorKind as LibraryErrorKind, Result as LibraryResult};
use crate::scan::Scan;
use crate::scan::error::{ErrorKind as ScanErrorKind, Result as ScanResult};
use crate::scan::file::scan_file_inner;
use async_stream::stream;
use exn::ResultExt;
use futures::{Stream, StreamExt};
use rawr_cache::Repository;
use rawr_storage::BackendHandle;
use std::path::{Path, PathBuf};
use std::pin::pin;

pub enum ScanEvent {
    Started,
    FileDiscovered { path: PathBuf },
    DiscoveryComplete(u64),
    Scanned(Box<Scan>),
    Complete,
}

pub fn scan<'a>(
    backend: &'a BackendHandle,
    cache: &'a Repository,
    prefix: Option<impl AsRef<Path>>,
) -> impl Stream<Item = LibraryResult<ScanEvent>> + 'a {
    // I've been using AsRef too much, and need to start using Into more.
    let prefix = prefix.map(|p| p.as_ref().to_path_buf());
    stream! {
        for await event in scan_inner(backend, cache, prefix) {
            yield event.or_raise(|| LibraryErrorKind::Scan);
        }
    }
}

fn scan_inner<'a>(
    backend: &'a BackendHandle,
    cache: &'a Repository,
    prefix: Option<PathBuf>,
) -> impl Stream<Item = ScanResult<ScanEvent>> + 'a {
    stream! {
        yield Ok(ScanEvent::Started);
        // Three options:
        // 1. We fetch all the files into memory first, then we can tell the
        //    caller "we found X files!" via ScanEvent::DiscoveryComplete(X).
        // 2. We process the files as soon as we know about them, but we won't
        //    know how many there are until we stop receiving more.
        // 3. We prioritise discovery so we can yield a DiscoveryComplete(X) as
        //    soon as possible BUT but we also process files in our queue while
        //    that's going on (yay tokio select!) DOUBLE BUT it's the most
        //    complex because streams are hard, yo.
        // ... guess which one we're implementing, kiddos!
        let mut file_stream = pin!(backend.list_stream(prefix.as_deref()));
        let mut discovery_complete = false;
        let mut discovered = 0u64;
        let mut processing = Vec::new();
        loop {
            tokio::select! {
                biased;
                file = file_stream.next(), if !discovery_complete => match file {
                    Some(Ok(file)) => {
                        discovered += 1;
                        todo!();
                        yield Ok(ScanEvent::FileDiscovered { path: file.path.clone });
                    },
                    Some(Err(e)) => yield Err(e).or_raise(|| ScanErrorKind::Storage),
                    None => {
                        discovery_complete = true;
                        yield Ok(ScanEvent::DiscoveryComplete(discovered));
                    }
                },
                // Well fuck, processing needs to return a future for tokio::select
                // to work. So much for "a queue of files will be easy!". Do I really
                // want to push thousands and thousands of futures onto the heap?
                // How many bytes do state machines require?? They could possibly
                // contain the bytes of the entire decompressed HTML file?! What
                // the hell do I use instead of a Vec? LeVecDeFutures::new()???
                Some(file) = processing.next(), if !processing.is_empty() => {
                    let path = file.path.clone();
                    yield scan_file_inner(backend, cache, file).await
                        .map(|s| ScanEvent::Scanned(Box::new(s)))
                        .or_raise(|| ScanErrorKind::ScanFailed(path));
                },
                else => {
                    // All done!
                    break;
                }
            }
        }
        yield Ok(ScanEvent::Complete);
    }
}
