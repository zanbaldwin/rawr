use super::File;
use super::context::ExportContext;
use crate::context::AppContext;
use crate::error::Result;
use crate::output::{Reason, format_pair_line};
use rawr_app::audio::{Need, codec, document, recordings, synthesiser};
use rawr_audiobook::plan::{BatchState, Plan};
use rawr_extract::models::Version;
use rawr_output::{Line, Loudness, PALETTE, Pipe};
use std::io::Cursor;
use std::process::ExitCode;

/// Build audiobooks from recordings that already exist. Sends no request and pays nothing.
pub(crate) async fn export(
    app: &AppContext,
    ctx: ExportContext<'_>,
    works: Vec<(Version, Vec<File>)>,
) -> Result<ExitCode> {
    let (provider, _) = synthesiser(&app.config, Need::Export)?;
    let store = recordings(&app.config, app.dry_run)?;
    let codec = codec(&app.config)?;

    let bar = ctx.output.progress_bar("Exporting");
    bar.set_length(works.len() as u64);
    let mut error_count: u64 = 0;
    let fail = |message: String| {
        ctx.output.print_to(
            Pipe::Err,
            &Line::new([("\u{2717} ", &PALETTE.danger).into(), (message,).into()]).with_volume(Loudness::Shout),
        );
    };

    'work: for (version, files) in works {
        let work = version.metadata.work_id;
        let Some((file, html)) = rawr_app::works::read_html(&ctx.load, files).await? else {
            error_count += 1;
            let line = format_pair_line(Reason::Failed, None::<&File>, &version, ctx.fandoms);
            ctx.output.print_to(Pipe::Err, &line);
            bar.inc(1);
            continue 'work;
        };
        let parsed = tokio::task::spawn_blocking(move || document(&html)).await.expect("document task panicked");
        let document = match parsed {
            Ok(document) => document,
            Err(error) => {
                error_count += 1;
                fail(format!("Work {work}: {}", describe(error.frame())));
                bar.inc(1);
                continue 'work;
            },
        };
        let plan = match Plan::new(&document, provider.as_ref(), &store).await {
            Ok(plan) => plan,
            Err(error) => {
                error_count += 1;
                fail(format!("Work {work}: {}", describe(error.frame())));
                bar.inc(1);
                continue 'work;
            },
        };
        let total = plan.batches().count();
        let missing = total - plan.count(BatchState::Recorded);
        if missing > 0 {
            error_count += 1;
            fail(format!(
                "Work {work}: {missing} of {total} batches have no usable recording with the current voice settings. Run `rawr narrate {work} --dry-run` to see what is missing."
            ));
            bar.inc(1);
            continue 'work;
        }
        let package = rawr_audiobook::export(&document, &plan, &store, &codec, &ctx.styles).await?;
        let bytes = package.write(Cursor::new(Vec::new()))?.into_inner();
        let path = ctx.path_generator.generate(&version, "epub", None)?;
        ctx.save.write(&path, &bytes).await?;
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
                (format!("{error_count} work(s) failed during export"),).into(),
            ])
            .with_volume(Loudness::Shout),
        );
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// An error and its first cause, and theirs, as one line.
fn describe(frame: &exn::Frame) -> String {
    match frame.children().first() {
        Some(child) => format!("{}: {}", frame.error(), describe(child)),
        None => frame.error().to_string(),
    }
}
