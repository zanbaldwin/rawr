use super::Command;
use crate::context::{AppContext, BackendPurpose};
use crate::error::Result;
use crate::output::util::{Reason, format_pair};
use crate::output::{Line, Loudness, PALETTE, Piece, Pipe};
use clap::Args;
use futures::StreamExt;
use rawr_library::scan::{ScanEffort, ScanEvent, scan};
use rawr_storage::BackendHandle;
use rawr_storage::backend::HtmlOnlyBackend;
use std::pin::pin;
use std::process::ExitCode;
use std::sync::Arc;

/// Discover HTML files in the import target and extract AO3 metadata.
///
/// Walks the configured import backend, identifies HTML files, extracts
/// work metadata (title, authors, fandoms, chapters, word count), and
/// caches the results in the local database. Previously scanned files
/// are skipped unless their content has changed.
#[derive(Debug, Args)]
#[command(after_long_help = "\
Examples:
  rawr scan
  rawr scan --dry-run")]
pub(crate) struct ScanCommand {}
impl Command for ScanCommand {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode> {
        let import_backend = ctx.get_backend_by_purpose(BackendPurpose::Import).await?.ok_or_else(|| {
            miette::miette!(
                "No import target configured. Define one in your config file under `library.targets.import`."
            )
        })?;
        let backend: BackendHandle = Arc::new(HtmlOnlyBackend::new(import_backend));
        let mut stream = pin!(scan(&backend, &ctx.cache, None::<&str>)?);

        let discovery = ctx.output.spinner("Discovering…");
        let bar = ctx.output.progress_bar("Scanning");

        let mut error_count: u64 = 0;

        while let Some(event) = stream.next().await {
            match event {
                Ok(ScanEvent::Started) => {},
                Ok(ScanEvent::FileDiscovered(_)) => discovery.inc(1),
                Ok(ScanEvent::DiscoveryComplete(total)) => {
                    discovery.finish_with_message(format!("Discovered {total} files"));
                    bar.set_length(total);
                },
                Ok(ScanEvent::Scanned(scan)) => {
                    let scan = scan.as_ref();
                    let reason = match scan.effort {
                        ScanEffort::Cached => Reason::Unchanged,
                        ScanEffort::Processed | ScanEffort::Recalculated => Reason::Added,
                    };
                    let line = format_pair(reason, &scan.file, &scan.version, &ctx.config.fandoms);
                    ctx.output.print(Pipe::Out, &line);
                    bar.inc(1);
                },
                Ok(ScanEvent::Complete) => {
                    discovery.finish_and_clear();
                    bar.finish();
                },
                Err(_) => {
                    error_count += 1;
                    bar.inc(1);
                },
            }
        }

        if error_count > 0 {
            ctx.output.print(
                Pipe::Err,
                &Line::new([
                    ("warning:", &PALETTE.warning).into(),
                    Piece::space(),
                    (format!("{error_count} file(s) failed during scan"),).into(),
                ])
                .with_volume(Loudness::Shout),
            );
        }
        Ok(ExitCode::SUCCESS)
    }
}
