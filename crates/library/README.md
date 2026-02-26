# Braindump / Scratchpad

```rust
use futures::AsyncReadExt;
use tokio_util::io::SyncIoBridge;
// It took me about 5 seconds to realise the reason rustfmt didn't automatically
// sort the following on Ctrl+S is because this is a Markdown file and now I
// shall leave it as a monument to my stupidity.
use rawr_extract::{extract, ESTIMATED_HEADER_SIZE_BYTES};

// GIVEN:
let source: FileInfo<Processed> = todo!();
let target_path: PathBuf = todo!();
let target_compression: Compression = todo!();

// WHEN compress is sync THEN:
let async_reader = backend.reader(&source.path).await?;
// Good fucking god. I think I've just discovered that, even if I add async
// reader/writer methods to the backend trait, compress being purely async is
// the WORST. I need to go back and add (gated?) async methods.
let (version, compressed_bytes) = tokio::task::spawn_blocking(move || {
    let sync_reader = SyncIoBridge::new(async_reader);
    let mut peekable = source.compression.peekable_reader(sync_reader)?;
    let version = rawr_extract::extract(peekable.peek(ESTIMATED_HEADER_SIZE_BYTES)?)?;
    // "I AM SYNCING! VHAT ARE YOU SYNCING ABOUT?" (in german accent) <-- the terrible
    // joke my coworker tells too often that I feel disappointed in myself for
    // laughing at every time.
    let mut output = Vec::new();
    let mut compressor = target_compression.wrap_writer(&mut output)?;
    peekable.copy_into(&mut compressor)?;
    drop(compressor); // flush and drop mutable borrow
    Ok((version, output))
}).await??;
backend.write(&target_path, &compressed_bytes).await?;

// WHEN compress is async THEN:
let mut peekable = source.compression.peekable_async_reader(backend.reader(&source.path).await?).await?;
let version = rawr_extract::extract(peekable.peek(ESTIMATED_HEADER_SIZE_BYTES).await?)?;
let mut compressor = target_compression.wrap_async_writer(backend.writer(&target_path).await?).await?;
peekable.copy_into_async(&mut compressor).await?;
compressor.shutdown().await?; // <-- yeah, I hate this quirk of async

// DONE:
println!("{}", target_path.display());
```

So yeah. Making the compress crate async would be great for DX, but... all the
implementations (flat2, brotli, zstd, etc) are all sync. There has to be a
sync/async boundary SOMEWHERE. Do I do it here in the library crate that
orchestrates everything, or do I kick it into the compress crate because it
knows about the sync implementations?

A quick Google let's me know that there's an `async-compression` crate. It
implements all the formats I currently have implemented. Am I ever going to want
an implementation that isn't already covered by `async-compression`? Would it
make my `rawr-compress` crate redundant? I think I'm going to ignore this for
the weekend and come back to it next week. There's too much brainthink happening
right now.
