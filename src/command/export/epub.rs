use super::File;
use super::context::ExportContext;
use crate::error::Result;
use crate::output::{Reason, format_pair_line};
use rawr_extract::Extractor;
use rawr_extract::models::Version;
use rawr_output::{Line, Loudness, PALETTE, Pipe};
use rawr_render::EpubRenderer;
use rawr_render::epub::EpubInput;
use std::process::ExitCode;
use std::sync::Arc;

pub(crate) async fn export(mut ctx: ExportContext<'_>, works: Vec<(Version, Vec<File>)>) -> Result<ExitCode> {
    let styles = std::mem::take(&mut ctx.styles);
    let renderer = Arc::new(EpubRenderer::new(styles));

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
            ctx.output.print_to(Pipe::Err, &line);
            bar.inc(1);
            continue 'work;
        };

        let compressed = ctx.load.read(&file.path).await?;
        let html = file.compression.decompress(&compressed)?;
        let metadata = version.metadata.clone();
        let export_path = ctx.path_generator.generate(&version, "epub", None)?;
        let renderer = Arc::clone(&renderer);
        let epub_temp = tokio::task::spawn_blocking(move || {
            let chapters = Extractor::from_html(&html).chapters_xhtml();
            renderer.render(&EpubInput { metadata, chapters })
        })
        .await
        .expect("render task panicked")?;
        let epub_bytes = tokio::fs::read(epub_temp.path())
            .await
            .map_err(|e| miette::miette!("Failed to read rendered EPUB: {e}"))?;
        ctx.save.write(&export_path, &epub_bytes).await?;
        let line = format_pair_line(Reason::Added, &file, &version, ctx.fandoms);
        ctx.output.print_to(Pipe::Out, &line);
        bar.inc(1);
    }

    bar.finish();

    if error_count > 0 {
        ctx.output.print_to(
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
