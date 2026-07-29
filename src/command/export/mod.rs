mod context;
#[cfg(feature = "epub")]
mod epub;
#[cfg(feature = "pdf")]
mod pdf;

use self::context::{ExportContext, WorkRef};
#[cfg(feature = "epub")]
use self::epub::export as export_epub;
#[cfg(feature = "pdf")]
use self::pdf::export as export_pdf;
use super::Command;
use crate::context::AppContext;
use crate::error::Result;
use crate::output::Picker;
use clap::Args;
use rawr_extract::models::Version;
use rawr_output::{Line, Loudness, PALETTE, Pipe};
use rawr_storage::file::{FileInfo, Processed};
use std::process::ExitCode;
use std::sync::Arc;

type File = FileInfo<Processed>;

const DEFAULT_LIMIT: usize = 25;

#[derive(Debug, Clone, clap::ValueEnum)]
enum ExportFormat {
    /// Render works to PDF using Chrome/Chromium
    #[cfg(feature = "pdf")]
    Pdf,
    /// Render works to EPUB
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
            Picker::interact(ctx.load.name(), Arc::clone(&ctx.cache), Arc::clone(&ctx.output), ctx.fandoms, limit)
                .await?
        } else {
            resolve_work_refs(&ctx, &self.works).await
        };

        match self.format {
            #[cfg(feature = "pdf")]
            ExportFormat::Pdf => export_pdf(ctx, selections).await,
            #[cfg(feature = "epub")]
            ExportFormat::Epub => export_epub(ctx, selections).await,
        }
    }
}

async fn resolve_work_refs(ctx: &ExportContext<'_>, works: &[WorkRef]) -> Vec<(Version, Vec<File>)> {
    let mut resolved = Vec::new();
    for work_ref in works {
        match resolve_one(ctx, work_ref).await {
            Ok(pair) => resolved.push(pair),
            Err(msg) => {
                ctx.output.print_to(
                    Pipe::Err,
                    &Line::new([("\u{2717} ", &PALETTE.danger).into(), (msg,).into()]).with_volume(Loudness::Shout),
                );
            },
        }
    }
    resolved
}

async fn resolve_one(ctx: &ExportContext<'_>, work_ref: &WorkRef) -> std::result::Result<(Version, Vec<File>), String> {
    match work_ref {
        WorkRef::BestWork(id) => ctx
            .cache
            .get_best_for_work_id(*id)
            .await
            .map_err(|e| format!("Work {id}: {e}"))?
            .ok_or_else(|| format!("Work {id} not found in library")),
        WorkRef::WorkVersion(id, crc) => ctx
            .cache
            .get_by_work_id(*id)
            .await
            .map_err(|e| format!("Work {id}: {e}"))?
            .into_iter()
            .find(|(version, _)| version.crc32 == *crc)
            .ok_or_else(|| format!("Work {id} has no version with CRC32 {crc:08x}")),
        WorkRef::FilePath(path) => {
            let target = ctx.load.name();
            ctx.cache
                .get_by_target_path(target, path.as_str())
                .await
                .map_err(|e| format!("\"{path}\": {e}"))?
                .map(|(file, version)| (version, vec![file]))
                .ok_or_else(|| format!("\"{path}\" not found in target \"{target}\""))
        },
    }
}
