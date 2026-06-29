use super::Command;
use crate::context::{AppContext, BackendPurpose};
use crate::error::{Error, Result};
use crate::output::{Reason, format_pair_line};
use async_stream::stream;
use clap::Args;
use directories::UserDirs;
use futures::{Stream, StreamExt};
use rawr_compress::Compression;
use rawr_compress::cli::Preference;
use rawr_library::RECOMMENDED_MAX_CONCURRENCY;
use rawr_library::import::{Import, import_file};
use rawr_output::{Line, Loudness, PALETTE, Piece, Pipe};
use rawr_storage::backend::is_html_path;
use std::path::PathBuf;
use std::pin::pin;
use std::process::ExitCode;
use std::sync::Arc;
use tokio::fs::File as AsyncFile;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::compat::TokioAsyncReadCompatExt;

/// Import HTML files from the local filesystem into the library.
///
/// Copies files into the configured import target, extracts metadata,
/// and caches the results. Source files are deleted after import by
/// default (use --keep to preserve them).
#[derive(Debug, Args)]
pub(crate) struct ImportCommand {
    /// File or folder to import. Defaults to the platform downloads directory.
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
    /// Recurse into subdirectories when importing a folder.
    #[arg(short, long)]
    recursive: bool,
    /// Override the configured compression format (eg, --compress=bz2).
    ///
    /// Formats: none, gz (gzip), bz2 (bzip2), br (brotli), bz3 (bzip3), xz (lzma), zst (zstd).
    /// Some formats require the corresponding feature flag at compile time.
    #[arg(long, conflicts_with = "no_compress")]
    compress: Option<String>,
    /// Disable compression, preserving the source file's format as-is.
    #[arg(long, conflicts_with = "compress")]
    no_compress: bool,
    /// Delete source files after successful import (default).
    #[arg(long, conflicts_with = "keep")]
    rm: bool,
    /// Keep source files after import.
    #[arg(long, conflicts_with = "rm")]
    keep: bool,
}

impl Command for ImportCommand {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode> {
        let compression = match Preference::from_flags(self.compress.clone(), self.no_compress)? {
            Preference::Explicit(c) => Some(c),
            Preference::Implicit => Some(ctx.config.library.compression),
            Preference::NotSpecified => None,
        };

        let lib_ctx = ctx.get_library_context(compression).await?;
        let import_backend = ctx.get_backend_by_purpose(BackendPurpose::Import).await?.ok_or_else(|| {
            miette::miette!(
                "No import target configured. Define one in your config file under `library.targets.import`."
            )
        })?;
        let source_path = match &self.path {
            Some(path) => path.clone(),
            None => UserDirs::new().and_then(|dirs| dirs.download_dir().map(|p| p.to_path_buf())).ok_or_else(|| {
                miette::miette!(
                    help = "Specify a path explicitly: rawr import <PATH>",
                    "Could not determine your downloads directory"
                )
            })?,
        };
        let import_path = source_path
            .canonicalize()
            .map_err(|e| miette::miette!("Cannot access '{}': {e}", source_path.display()))?;
        // TODO: Make sure that import path does not overlap the import backend.

        let mut file_stream = pin!(discover_files(import_path, self.recursive)?);

        let discovery = ctx.output.spinner("Discovering\u{2026}");
        let bar = ctx.output.progress_bar("Importing");

        let should_delete = !self.keep && !ctx.dry_run;
        let semaphore = Arc::new(Semaphore::new(RECOMMENDED_MAX_CONCURRENCY));
        let mut tasks = JoinSet::new();
        let mut discovery_complete = false;
        let mut discovered = 0u64;
        let mut error_count: u64 = 0;

        loop {
            tokio::select! {
                biased;

                path = file_stream.next(), if !discovery_complete => match path {
                    Some(source_path) => {
                        discovery.inc(1);
                        let backend = Arc::clone(&import_backend);
                        let cache = ctx.cache.clone();
                        let lib_ctx = Arc::clone(&lib_ctx);
                        let semaphore = Arc::clone(&semaphore);
                        tasks.spawn(async move {
                            let _permit = semaphore.acquire().await.expect("semaphore closed");
                            let source_compression = Compression::from_path(&source_path);
                            let reader = AsyncFile::open(&source_path)
                                .await
                                .map_err(|e| miette::miette!("Failed to open '{}': {e}", source_path.display()))?
                                .compat();
                            let import = import_file(&backend, &cache, &lib_ctx, source_compression, reader).await?;
                            Ok::<_, Error>((source_path, import))
                        });
                    },
                    None => {
                        discovery_complete = true;
                        discovered = discovery.position();
                        discovery.finish_with_message(format!("Discovered {discovered} files"));
                        bar.set_length(discovered);
                    },
                },

                Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                    match result.expect("import task panicked") {
                        Ok((source_path, import)) => {
                            let (reason, file, version) = match &import {
                                Import::NewImport(f, v) => (Reason::Added, f, v),
                                Import::Upgrade(f, v) => (Reason::Upgraded, f, v),
                                Import::AlreadyExists(f, v) => (Reason::Unchanged, f, v),
                                Import::Outdated(f, v) => (Reason::Outdated, f, v),
                            };
                            let line = format_pair_line(reason, file, version, &ctx.config.fandoms);
                            ctx.output.print_to(Pipe::Out, &line);
                            if should_delete && let Err(e) = tokio::fs::remove_file(&source_path).await {
                                tracing::info!(path = %source_path.display(), error = %e, "failed to delete source file");
                            }
                        },
                        Err(_) => {
                            error_count += 1;
                        },
                    }
                    bar.inc(1);
                },

                else => break,
            }
        }

        discovery.finish_and_clear();
        bar.finish();

        if discovered == 0 {
            ctx.output.print_to(
                Pipe::Err,
                &Line::new([Piece::fixed("No importable files found", &PALETTE.warning)]).with_volume(Loudness::Shout),
            );
            return Ok(ExitCode::SUCCESS);
        }

        if error_count > 0 {
            ctx.output.print_to(
                Pipe::Err,
                &Line::new([
                    ("WARN: ", &PALETTE.warning).into(),
                    (format!("{error_count} file(s) failed during import"),).into(),
                ])
                .with_volume(Loudness::Shout),
            );
        }
        Ok(ExitCode::SUCCESS)
    }
}

// TODO: Create stream_discovered_files() function that recreates the streaming
//       functionality of rawr_library::scan::scan(), even re-using the
//       ScanEvent enum as its return type.
// TODO: Identify what functionality is common between the two
//       (scan/stream_discovered_files) and what should remain independeny, and
//       write comprehensive comments ready to extract it in a future epic.

/// Discover files on the local filesystem that are import candidates (HTML files).
fn discover_files(path: PathBuf, recursive: bool) -> Result<impl Stream<Item = PathBuf>> {
    if path.is_file() && !is_html_path(&path) {
        Err(miette::miette!("'{}' is not a HTML file", path.display()))?;
    } else if !path.is_file() && !path.is_dir() {
        Err(miette::miette!("'{}' is not a file or directory", path.display()))?;
    }
    Ok(stream! {
        if path.is_file() {
            yield path;
            return;
        }
        let mut stack = vec![path];
        while let Some(dir) = stack.pop() {
            let mut entries = match tokio::fs::read_dir(&dir).await {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(path = %dir.display(), error = %e, "failed to read directory");
                    continue;
                },
            };
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_file() && is_html_path(&path) {
                    yield path;
                } else if path.is_dir() && recursive {
                    stack.push(path);
                }
            }
        }
    })
}
