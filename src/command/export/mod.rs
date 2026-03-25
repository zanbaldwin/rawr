mod context;
#[cfg(feature = "epub")]
mod epub;
#[cfg(feature = "pdf")]
mod pdf;

#[cfg(feature = "epub")]
use self::epub::export as export_epub;
#[cfg(feature = "pdf")]
use self::pdf::export as export_pdf;
use super::Command;
use crate::error::Result;
use crate::{command::export::context::WorkRef, context::AppContext};
use clap::Args;
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
        let _limit = self.show.unwrap_or(DEFAULT_LIMIT);

        let selections = vec![];
        if selections.is_empty() {
            return Ok(ExitCode::SUCCESS);
        }

        match self.format {
            #[cfg(feature = "pdf")]
            ExportFormat::Pdf => export_pdf(ctx, selections).await,
            #[cfg(feature = "epub")]
            ExportFormat::Epub => export_epub(ctx, selections).await,
        }
    }
}
