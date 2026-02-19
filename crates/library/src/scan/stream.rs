use crate::error::{ErrorKind as LibraryErrorKind, Result as LibraryResult};
use crate::scan::Scan;
use crate::scan::error::{ErrorKind as ScanErrorKind, Result as ScanResult};
use crate::scan::file::scan_file_inner;
use async_stream::stream;
use exn::ResultExt;
use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt};
use rawr_cache::Repository;
use rawr_storage::BackendHandle;
use std::path::{Path, PathBuf};
use std::pin::pin;

const MAX_PROCESS_CONCURRENCY: usize = 100;

pub enum ScanEvent {
    Started,
    FileDiscovered(PathBuf),
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
        // ... guess which one we're gonna implement, kiddos!

        // Well fuck, processing needs to return a future for tokio::select
        // to work. So much for "a queue of files will be easy!". Do I really
        // want to push thousands and thousands of futures onto the heap?
        // How many bytes do state machines require?? They could possibly
        // contain the bytes of the entire decompressed HTML file?! With
        // thousands of files at several megabytes that would cause a heap
        // overflow. Plus, what the hell do I use instead of a Vec?
        // LeVecDeFutures::new()???

        // Oh. LeVecDeFutures wasn't actually far off. `futures::stream::FuturesUnordered`
        // exists and is literally a collection of futures. Damn, I'm good.
        // Oh, and I was wrong about heap size: it'll keep growing into swap. But a
        // ulimit could be set and that's essentially the same thing as max heap size.
        // Either way, let's not request 10's of GB of heap, shall we?

        // Okay, so I thought `FuturesUnordered::buffer_unordered(MAX_PROCESS_CONCURRENCY)`
        // was a super-convenient perfect way forward, but `futures::stream::BufferUnordered`
        // doesn't allow adding more futures to it once it's been constructed, so we won't
        // be able to process files before discovery is complete (Option 1).
        // Adding `if !discovery_complete && processing.len() < MAX_PROCESS_CONCURRENCY`
        // guard clause to the select arm is basically the same as Option 2. It's not the end
        // of the world, but there'd be no point in having progress bars in the TUI. And I
        // want my goddamn progress bars!
        // So we need a buffer of our buffer. A process queue for our process queue?
        // And we have to make sure that it doesn't start/poll the futures, so I was
        // right all along: we need a LeVecDeFutures!! I am not looking forward to figuring
        // out the type signature of that.

        // This is ending up waaay harder than I thought it was going to be, I'm neck-deep
        // in docs.rs pages, and this entire time VSCode has highlighted this entire fucking
        // function as "unreachable statement" so I don't even get IDE completions while writing it.

        // The code compiles! The LSP is giving me type information again! Hallelujah!
        // I've got `processing` (max: 100) and `not_processing_yet`. When a file is
        // processed it'll grab another future from the "not yet" pile into the queue.
        // The code compiles, but it seems too easy. There's a bug in here somewhere, I know it.

        // Refactor the existing code and structs and enums to make the code pretty
        // because I'm procrastinating... I'm so used to Clippy telling me I'm a
        // dumb dumb that I don't know what to do when it doesn't say anything.

        // Add a "just covering my ass" clause in the else block.
        // I think this is now done, but I've still got this nagging feeling that
        // I'm missing something. Guess I'll come back to this later on when I
        // start encountering bugs in the main CLI application...

        let mut file_stream = pin!(backend.list_stream(prefix.as_deref()));
        let mut discovery_complete = false;
        let mut discovered = 0u64;
        let mut not_processing_yet = Vec::new();
        let mut processing = FuturesUnordered::new();
        loop {
            tokio::select! {
                biased;

                file = file_stream.next(), if !discovery_complete => match file {
                    Some(Ok(file)) => {
                        discovered += 1;
                        let path = file.path.clone();
                        // TODO is the initial state of the future at the "here are the
                        // arguments to the function, and the function body is ready to
                        // execute on next poll" OR "everything up to the first .await call"?
                        // Because that could potentially change the size of elements
                        // in `not_processing_yet` if there are sync operations between
                        // function call and first await?
                        let future = scan_file_inner(backend, cache, file);
                        if processing.len() < MAX_PROCESS_CONCURRENCY {
                            processing.push(future);
                        } else {
                            not_processing_yet.push(future);
                        }
                        yield Ok(ScanEvent::FileDiscovered(path));
                    },
                    Some(Err(e)) => yield Err(e).or_raise(|| ScanErrorKind::Storage),
                    None => {
                        discovery_complete = true;
                        yield Ok(ScanEvent::DiscoveryComplete(discovered));
                    }
                },

                Some(result) = processing.next(), if !processing.is_empty() => {
                    yield result
                        .map(|s| ScanEvent::Scanned(Box::new(s)))
                        .or_raise(|| ScanErrorKind::ScanFailed);
                    if let Some(future) = not_processing_yet.pop() {
                        processing.push(future);
                    }
                },

                else => {
                    // DONE: Discovery is complete.
                    // DONE: `processing` queue is empty.
                    // `not_processing_yet` might not be empty? In what scenario would
                    // that occur? Personally I don't think this is possible, but that
                    // doesn't mean I'm confident about it.
                    if !not_processing_yet.is_empty() {
                        let yet_to_process = not_processing_yet.len();
                        let batch = MAX_PROCESS_CONCURRENCY.min(yet_to_process);
                        processing.extend(not_processing_yet.drain(..batch));
                    } else {
                        // All done!
                        break;
                    }
                },
            }
        }
        yield Ok(ScanEvent::Complete);
    }
}
