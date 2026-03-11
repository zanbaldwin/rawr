use super::Command;
use crate::command::format_processed;
use crate::context::{AppContext, BackendPurpose};
use crate::error::Result;
use clap::Args;
use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rawr_library::scan::{ScanEvent, scan};
use rawr_storage::BackendHandle;
use rawr_storage::backend::HtmlOnlyBackend;
use std::pin::pin;
use std::process::ExitCode;
use std::sync::Arc;

#[derive(Debug, Args)]
pub(crate) struct ScanCommand {}
impl Command for ScanCommand {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode> {
        // When scanning, we don't want to pick up files that have nothing to do with AO3 downloads.
        // Eg, we don't want to present errors to the user if they choose to place their config in the same directory.
        let import_backend = ctx.get_backend_by_purpose(BackendPurpose::Import).await?.ok_or_else(|| {
            miette::miette!(
                "No import target configured. Define one in your config file under `library.targets.import`."
            )
        })?;
        let backend: BackendHandle = Arc::new(HtmlOnlyBackend::new(import_backend));
        let mut stream = pin!(scan(&backend, &ctx.cache, None::<&str>)?);
        // Progress bars.
        let multi = MultiProgress::new();

        let discovery_bar = multi.add(ProgressBar::new_spinner());
        discovery_bar.enable_steady_tick(std::time::Duration::from_millis(100));
        discovery_bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg} [{elapsed_precise}] {pos} files found")
                .expect("valid template"),
        );
        discovery_bar.set_message("Discovering…");

        let scan_bar = multi.add(ProgressBar::new(0));
        scan_bar.enable_steady_tick(std::time::Duration::from_millis(100));
        scan_bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} Scanning [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) [{elapsed_precise}]")
                .expect("valid template"),
        );

        let mut error_count: u64 = 0;

        while let Some(event) = stream.next().await {
            match event {
                Ok(ScanEvent::Started) => {},
                Ok(ScanEvent::FileDiscovered(_path)) => {
                    discovery_bar.inc(1);
                },
                Ok(ScanEvent::DiscoveryComplete(total)) => {
                    discovery_bar.finish_with_message(format!("Discovered {total} files"));
                    scan_bar.set_length(total);
                },
                Ok(ScanEvent::Scanned(scan)) => {
                    let scan = scan.as_ref();
                    scan_bar.println(format_processed(ctx, &scan.version.metadata, Some(&scan.file.path)));
                    scan_bar.inc(1);
                },
                Ok(ScanEvent::Complete) => {
                    discovery_bar.finish();
                    scan_bar.finish();
                },
                Err(_) => {
                    //tracing::warn!("{e}");
                    error_count += 1;
                    scan_bar.inc(1);
                },
            }
        }

        multi.clear().ok();

        if error_count > 0 {
            tracing::warn!("{error_count} file(s) failed during scan");
        }
        Ok(ExitCode::SUCCESS)
    }
}
