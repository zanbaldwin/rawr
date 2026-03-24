mod context;
#[cfg(feature = "epub")]
mod epub;
#[cfg(feature = "pdf")]
mod pdf;

pub(crate) use self::context::ExportContext;
#[cfg(feature = "epub")]
use self::epub::export as export_epub;
#[cfg(feature = "pdf")]
use self::pdf::export as export_pdf;
use super::Command;
use crate::command::export::picker::select_interactively;
use crate::error::Result;
use crate::output::{Line, Loudness, PALETTE, Pipe};
use crate::{command::export::context::WorkRef, context::AppContext};
use clap::Args;
use rawr_extract::models::Version;
use rawr_storage::file::{FileInfo, Processed};
use std::process::ExitCode;

type File = FileInfo<Processed>;

const DEFAULT_LIMIT: usize = 25;

#[derive(Debug, Clone, clap::ValueEnum)]
enum ExportFormat {
    /// Render works to PDF using Chrome/Chromium
    #[cfg(feature = "pdf")]
    Pdf,
    /// Render works to EPUB (not yet implemented)
    #[cfg(feature = "epub")]
    Epub,
}

/// Render works to PDF or EPUB.
#[derive(Debug, Args)]
pub(crate) struct ExportCommand {
    format: ExportFormat,
    /// Works to export. Accepts work IDs, work_id@hash, or file paths.
    /// If omitted, launches interactive picker.
    works: Vec<WorkRef>,
    /// Number of recent works to show in interactive mode.
    #[arg(long, default_value = "25")]
    show: Option<usize>,
}
impl Command for ExportCommand {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode> {
        let ctx = ExportContext::try_from_app(ctx).await?;
        let limit = self.show.unwrap_or(DEFAULT_LIMIT);

        let selections = if self.works.is_empty() {
            select_interactively(&ctx, limit).await?
        } else {
            resolve_work_refs(&ctx, &self.works).await
        };
        if selections.is_empty() {
            return Ok(ExitCode::SUCCESS);
        }

        match &self.format {
            #[cfg(feature = "pdf")]
            ExportFormat::Pdf => export_pdf(ctx, selections).await,
            #[cfg(feature = "epub")]
            ExportFormat::Epub => export_epub(ctx, selections).await,
        }
    }
}

async fn resolve_work_refs(ctx: &ExportContext<'_>, works: &[WorkRef]) -> Vec<(File, Version)> {
    let target = ctx.load.name();
    let mut resolved = Vec::new();
    for work_ref in works {
        match resolve_one(ctx, target, work_ref).await {
            Ok(pair) => resolved.push(pair),
            Err(msg) => {
                ctx.output.print(
                    Pipe::Err,
                    &Line::new([("\u{2717} ", &PALETTE.danger).into(), (msg,).into()]).with_volume(Loudness::Shout),
                );
            },
        }
    }
    resolved
}

async fn resolve_one(
    ctx: &ExportContext<'_>,
    target: &str,
    work_ref: &WorkRef,
) -> std::result::Result<(File, Version), String> {
    match work_ref {
        WorkRef::BestWork(id) => {
            let Some((version, files)) =
                ctx.cache.get_best_for_work_id(*id).await.map_err(|e| format!("Work {id}: {e}"))?
            else {
                return Err(format!("Work {id} not found in library"));
            };
            let file = files
                .into_iter()
                .find(|f| f.target == target)
                .ok_or_else(|| format!("Work {id} has no files in target \"{target}\""))?;
            Ok((file, version))
        },
        WorkRef::WorkVersion(id, crc) => {
            let versions = ctx.cache.get_by_work_id(*id).await.map_err(|e| format!("Work {id}: {e}"))?;
            for (version, files) in versions {
                if version.crc32 == *crc {
                    let file = files
                        .into_iter()
                        .find(|f| f.target == target)
                        .ok_or_else(|| format!("Work {id}@{crc:08x} has no files in target \"{target}\""))?;
                    return Ok((file, version));
                }
            }
            Err(format!("Work {id} has no version with CRC32 {crc:08x}"))
        },
        WorkRef::FilePath(path) => {
            let Some((file, version)) =
                ctx.cache.get_by_target_path(target, path.as_str()).await.map_err(|e| format!("\"{path}\": {e}"))?
            else {
                return Err(format!("\"{path}\" not found in target \"{target}\""));
            };
            Ok((file, version))
        },
    }
}
