use super::Command;
use crate::context::{AppContext, BackendPurpose};
use crate::error::Result;
use crate::output::{Line, Loudness, PALETTE, Piece, Pipe};
use clap::Args;
use rawr_compress::Compression;
use rawr_compress::cli::Preference;
use rawr_config::models::FandomConfig;
use rawr_extract::models::Version;
use rawr_library::import::{Import, import_file};
use rawr_storage::file::{FileInfo, Processed};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const MAX_PROCESS_CONCURRENCY: usize = 50;
const AVG_TITLE_LEN: usize = 28;

#[derive(Debug, Args)]
pub(crate) struct ImportCommand {
    /// File or folder to import.
    #[arg(value_name = "PATH")]
    path: PathBuf,
    /// Recurse into subdirectories when importing a folder.
    #[arg(short, long)]
    recursive: bool,
    /// Compress files during import. Optionally specify format (eg, --compress=bz2).
    /// Without a value, uses the configured default. Omit to preserve source compression.
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    compress: Option<String>,
    /// Delete source files after successful import (default).
    #[arg(long, conflicts_with = "keep")]
    rm: bool,
    /// Keep source files after import.
    #[arg(long, conflicts_with = "rm")]
    keep: bool,
}

impl Command for ImportCommand {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode> {
        let flag: rawr_compress::cli::Flag = self.compress.clone().map(|s| if s.is_empty() { None } else { Some(s) });
        let compression = match Preference::try_from(flag)? {
            Preference::Explicit(c) => Some(c),
            Preference::Implicit => Some(ctx.config.library.compression),
            Preference::NotSpecified => None,
        };

        let import_backend = ctx.get_backend_by_purpose(BackendPurpose::Import).await?.ok_or_else(|| {
            miette::miette!(
                "No import target configured. Define one in your config file under `library.targets.import`."
            )
        })?;

        let lib_ctx = ctx.get_library_context(compression).await?;

        let files = discover_files(&self.path, self.recursive)?;
        if files.is_empty() {
            ctx.output.print(
                Pipe::Err,
                &Line::new([Piece::fixed("No importable files found", &PALETTE.warning)]).with_volume(Loudness::Shout),
            );
            return Ok(ExitCode::SUCCESS);
        }

        let bar = ctx.output.progress_bar("Importing");
        bar.set_length(files.len() as u64);

        let should_delete = !self.keep && !ctx.dry_run;
        let semaphore = Arc::new(Semaphore::new(MAX_PROCESS_CONCURRENCY));
        let mut tasks = JoinSet::new();

        for source_path in files {
            let backend = import_backend.clone();
            let cache = ctx.cache.clone();
            let lib_ctx = Arc::clone(&lib_ctx);
            let semaphore = Arc::clone(&semaphore);
            tasks.spawn(async move {
                let _permit = semaphore.acquire().await.expect("semaphore closed");
                let source_compression = Compression::from_path(&source_path);
                let data = tokio::fs::read(&source_path)
                    .await
                    .map_err(|e| miette::miette!("Failed to read '{}': {e}", source_path.display()))?;
                let cursor = futures::io::Cursor::new(data);
                let import = import_file(&backend, &cache, &lib_ctx, source_compression, cursor).await?;
                Ok::<_, crate::error::Error>((source_path, import))
            });
        }

        let mut error_count: u64 = 0;
        while let Some(result) = tasks.join_next().await {
            match result.expect("import task panicked") {
                Ok((source_path, import)) => {
                    let line = format_import_line(&import, &ctx.config.fandoms);
                    ctx.output.print(Pipe::Out, &line);
                    if should_delete {
                        if let Err(e) = tokio::fs::remove_file(&source_path).await {
                            tracing::warn!(path = %source_path.display(), error = %e, "failed to delete source file");
                        }
                    }
                },
                Err(_) => {
                    error_count += 1;
                },
            }
            bar.inc(1);
        }

        bar.finish();

        if error_count > 0 {
            ctx.output.print(
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

fn discover_files(path: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    let path = path.canonicalize().map_err(|e| miette::miette!("Cannot access '{}': {e}", path.display()))?;

    if path.is_file() {
        return if is_importable(&path) {
            Ok(vec![path])
        } else {
            Err(miette::miette!("'{}' is not an importable HTML file", path.display()))?
        };
    }

    if !path.is_dir() {
        return Err(miette::miette!("'{}' is not a file or directory", path.display()))?;
    }

    let mut files = Vec::new();
    collect_files(&path, recursive, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(dir: &Path, recursive: bool, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| miette::miette!("Cannot read directory '{}': {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| miette::miette!("Error reading directory entry: {e}"))?;
        let path = entry.path();
        if path.is_file() && is_importable(&path) {
            files.push(path);
        } else if path.is_dir() && recursive {
            collect_files(&path, recursive, files)?;
        }
    }
    Ok(())
}

fn is_importable(path: &Path) -> bool {
    let compression = Compression::from_path(path);
    let check_path =
        if compression != Compression::None { Path::new(path.file_stem().unwrap_or_default()) } else { path };
    check_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm"))
}

fn format_import_line<'a>(import: &'a Import, fandoms: &'a FandomConfig) -> Line<'a> {
    let (prefix, prefix_style, file, version, loudness) = match import {
        Import::Imported(f, v) => ("+ ", &PALETTE.added, f, v, Loudness::Normal),
        Import::AlreadyExists(f, v) => ("= ", &PALETTE.muted, f, v, Loudness::Whisper),
        Import::Outdated(f, v) => ("\u{2193} ", &PALETTE.warning, f, v, Loudness::Normal),
    };
    format_line(prefix, prefix_style, file, version, fandoms).with_volume(loudness)
}

fn format_line<'a>(
    prefix: &'a str,
    prefix_style: &'a console::Style,
    file: &'a FileInfo<Processed>,
    version: &'a Version,
    fandoms: &'a FandomConfig,
) -> Line<'a> {
    let default_fandom = version.metadata.fandoms.first().map(|f| f.name.as_str());
    let fandom = fandoms.preferred_fandom(&version.metadata.fandoms).or(default_fandom);
    let chapters = version.metadata.chapters.to_string();
    [
        (prefix, prefix_style).into(),
        (format!("#{}", version.metadata.work_id), &PALETTE.highlight).into(),
        Piece::space(),
        (version.metadata.title.as_str(), &PALETTE.success, AVG_TITLE_LEN).into(),
        Piece::space(),
        (fandom.unwrap_or_default(), &PALETTE.warning).into(),
        Piece::space(),
        (format!("{:.1}k", version.metadata.words as f32 / 1000.0), &PALETTE.highlight).into(),
        Piece::space(),
        (chapters, if version.metadata.chapters.is_complete() { &PALETTE.success } else { &PALETTE.danger }).into(),
        Piece::space(),
        (file.path.as_str(), &PALETTE.muted, 32).into(),
    ]
    .into()
}
