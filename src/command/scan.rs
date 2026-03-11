use super::Command;
use crate::command::stylize_metadata;
use crate::context::{AppContext, BackendPurpose};
use crate::error::Result;
use crate::output::{Line, Loudness, Palette, Piece, Pipe};
use clap::Args;
use futures::StreamExt;
use rawr_library::scan::{ScanEffort, ScanEvent, scan};
use rawr_storage::BackendHandle;
use rawr_storage::backend::HtmlOnlyBackend;
use std::pin::pin;
use std::process::ExitCode;
use std::sync::Arc;

#[derive(Debug, Args)]
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
                    let pieces = stylize_metadata(&scan.version.metadata, Some(&scan.file.path));
                    let loudness = match scan.effort {
                        ScanEffort::Cached => Loudness::Quiet,
                        ScanEffort::Processed | ScanEffort::Recalculated => Loudness::Normal,
                    };
                    ctx.output.print(&Line::from_pieces(Pipe::Out, loudness, pieces));
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
            let palette = Palette::default();
            ctx.output.print(&Line::from_pieces(
                Pipe::Err,
                Loudness::Loud,
                [
                    ("warning:", &palette.warning).into(),
                    Piece::space(),
                    (format!("{error_count} file(s) failed during scan"),).into(),
                ],
            ));
        }
        Ok(ExitCode::SUCCESS)
    }
}
