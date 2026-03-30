use super::File;
use super::context::ExportContext;
use crate::error::Result;
use crate::output::util::{Reason, format_pair_line};
use crate::output::{Line, Loudness, PALETTE, Pipe};
use rawr_extract::models::Version;
use rawr_render::{CssVariables, PdfRenderer};
use std::process::ExitCode;
use std::sync::Arc;

pub(crate) async fn export(mut ctx: ExportContext<'_>, works: Vec<(Version, Vec<File>)>) -> Result<ExitCode> {
    let styles = std::mem::take(&mut ctx.styles);
    let renderer = Arc::new(PdfRenderer::new(styles)?);

    let bar = ctx.output.progress_bar("Exporting");
    bar.set_length(works.len() as u64);
    let mut error_count: u64 = 0;

    'work: for (version, files) in works {
        let mut selected = None;
        'find: for f in files {
            if ctx.load.exists(&f.path).await.is_ok_and(|b| b) {
                selected = Some(f);
                break 'find;
            }
        }

        let Some(file) = selected else {
            error_count += 1;
            let line = format_pair_line(Reason::Failed, None::<&File>, &version, ctx.fandoms);
            ctx.output.print(Pipe::Err, &line);
            bar.inc(1);
            continue 'work;
        };

        let compressed = ctx.load.read(&file.path).await?;
        let html = file.compression.decompress(&compressed)?;
        let vars = CssVariables::from(&version.metadata);
        let renderer = Arc::clone(&renderer);
        let pdf_temp = tokio::task::spawn_blocking(move || renderer.render_slice(&html, Some(vars)))
            .await
            .expect("render task panicked")?;
        let pdf_bytes =
            tokio::fs::read(pdf_temp.path()).await.map_err(|e| miette::miette!("Failed to read rendered PDF: {e}"))?;
        let export_path = ctx.path_generator.generate(&version, "pdf", None)?;
        ctx.save.write(&export_path, &pdf_bytes).await?;
        let line = format_pair_line(Reason::Added, &file, &version, ctx.fandoms);
        ctx.output.print(Pipe::Out, &line);
        bar.inc(1);
    }

    bar.finish();

    if error_count > 0 {
        ctx.output.print(
            Pipe::Err,
            &Line::new([
                ("WARN: ", &PALETTE.warning).into(),
                (format!("{error_count} file(s) failed during export"),).into(),
            ])
            .with_volume(Loudness::Shout),
        );
    }

    Ok(ExitCode::SUCCESS)
}
