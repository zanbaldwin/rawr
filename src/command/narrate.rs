use super::Command;
use super::export::audiobook::{describe, save};
use super::export::{ExportContext, select_works};
use crate::context::AppContext;
use crate::error::Result;
use crate::output::{Reason, format_pair_line};
use clap::Args;
use rawr_app::audio::{KeySource, Need, codec, document, recordings, synthesiser};
use rawr_app::works::{File, WorkRef};
use rawr_audiobook::epub3::narrator_credit;
use rawr_audiobook::narrate::{NarrationEvent, NarrationOptions, StopReason, narrate};
use rawr_audiobook::plan::{BatchState, Plan};
use rawr_config::models::StyleFormat;
use rawr_extract::models::Version;
use rawr_output::{Line, Loudness, Output, PALETTE, Pipe};
use rawr_source::{Document, Rating};
use rawr_tts::error::ErrorKind as TtsErrorKind;
use rawr_tts::{Cost, CostUnit};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio_util::sync::CancellationToken;

/// Narrate works with the configured text-to-speech provider, then export them as audiobooks.
///
/// Shows an estimate and asks before it pays. `--dry-run` shows the estimate only.
#[derive(Debug, Args)]
pub(crate) struct NarrateCommand {
    /// Works to narrate. Accepts work IDs, work_id@hash, or file paths.
    /// If omitted, launches interactive picker.
    works: Vec<WorkRef>,
    /// Number of recent works to show in interactive mode.
    #[arg(long, default_value = "25")]
    show: Option<usize>,
    /// Do not ask for confirmation before paying.
    #[arg(short = 'y', long)]
    yes: bool,
    /// Stop before sending more than this many characters in this run (all works together).
    #[arg(long, value_name = "N")]
    max_chars: Option<usize>,
    /// Approve sending works whose rating the provider restricts (for ElevenLabs: Explicit and Not Rated).
    #[arg(long)]
    approve: bool,
    /// Pay again for batches whose recording had an alignment mismatch.
    #[arg(long)]
    retry_mismatched: bool,
    /// Do not export the audiobook after recording.
    #[arg(long)]
    no_export: bool,
    /// Requests at the same time (default: the provider's).
    #[arg(long, value_name = "N")]
    concurrency: Option<usize>,
    /// Narrate in this language (a language code with a configured voice), whatever the work says.
    #[arg(long, value_name = "CODE")]
    language: Option<String>,
}

/// One work, planned.
struct Planned {
    version: Version,
    file: File,
    document: Document,
    plan: Plan,
}

impl Planned {
    fn work(&self) -> u64 {
        self.version.metadata.work_id
    }

    /// Characters this run would pay for.
    fn to_pay(&self, retry_mismatched: bool) -> (usize, usize) {
        self.plan
            .batches()
            .filter(|batch| match batch.state {
                BatchState::Missing => true,
                BatchState::Mismatched => retry_mismatched,
                BatchState::Recorded => false,
            })
            .fold((0, 0), |(batches, chars), batch| (batches + 1, chars + batch.chars))
    }
}

impl Command for NarrateCommand {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode> {
        let app: &AppContext = ctx;
        let ctx = ExportContext::try_from_app(app, StyleFormat::Audiobook).await?;
        let output = Arc::clone(&ctx.output);
        let need = if app.dry_run { Need::Export } else { Need::Narrate };
        let (provider, key_source) = synthesiser(&app.config, need)?;
        if key_source != KeySource::None {
            print(&*output, Pipe::Err, muted(format!("API key: {key_source}")).with_volume(Loudness::Whisper));
        }
        // Never narrate into a read-only store: in dry-run, nothing is narrated.
        let store = recordings(&app.config, app.dry_run)?;
        let codec = codec(&app.config)?;
        let max_chars = self.max_chars.or(app.config.audio.as_ref().and_then(|audio| audio.max_chars));

        let mut failed = 0usize;
        let mut works = Vec::new();
        for (version, files) in select_works(&ctx, &self.works, self.show).await? {
            let work = version.metadata.work_id;
            let Some((file, html)) = rawr_app::works::read_html(&ctx.load, files).await? else {
                failed += 1;
                print(&*output, Pipe::Err, format_pair_line(Reason::Failed, None::<&File>, &version, ctx.fandoms));
                continue;
            };
            let parsed = tokio::task::spawn_blocking(move || document(&html)).await.expect("document task panicked");
            let mut document = match parsed {
                Ok(document) => document,
                Err(error) => {
                    failed += 1;
                    print(&*output, Pipe::Err, danger(format!("Work {work}: {}", describe(error.frame()))));
                    continue;
                },
            };
            if let Some(language) = &self.language {
                document.meta.language = Some(language.clone());
            }
            match Plan::new(&document, provider.as_ref(), &store).await {
                Ok(plan) => works.push(Planned { version, file, document, plan }),
                Err(error) => {
                    failed += 1;
                    print(&*output, Pipe::Err, danger(format!("Work {work}: {}", describe(error.frame()))));
                },
            }
        }
        if works.is_empty() {
            return Ok(ExitCode::FAILURE);
        }

        // Estimate.
        let engine = narrator_credit(&works[0].plan.provider, &works[0].plan.fingerprint, None);
        let provider_name = engine.split(' ').next().unwrap_or(&engine).to_string();
        let (mut total_batches, mut total_chars, mut blocked) = (0, 0, 0);
        for planned in &works {
            let plan = &planned.plan;
            let (batches, chars) = planned.to_pay(self.retry_mismatched);
            let needs_approval = plan.needs_approval && !self.approve;
            let mismatched = plan.count(BatchState::Mismatched);
            let mut text = format!(
                "#{} \"{}\" · {} · {} · {} section(s) · {} batches: {} recorded, {} to record ({} characters)",
                planned.work(),
                plan.title,
                rating_name(plan.rating),
                plan.language.as_deref().unwrap_or("unknown language"),
                plan.sections.len(),
                plan.batches().count(),
                plan.count(BatchState::Recorded),
                batches,
                number(chars),
            );
            if mismatched > 0 && !self.retry_mismatched {
                text.push_str(&format!(", {mismatched} mismatched (--retry-mismatched pays again)"));
            }
            if needs_approval {
                blocked += 1;
                text.push_str(" · needs --approve");
                print(&*output, Pipe::Out, warning(text));
            } else {
                total_batches += batches;
                total_chars += chars;
                print(&*output, Pipe::Out, Line::new([(text,).into()]));
            }
        }
        let mut summary = format!("To record: {total_batches} batches · {} characters ({engine})", number(total_chars));
        if let Some(limit) = max_chars {
            summary.push_str(&format!(" · limit {} characters this run", number(limit)));
        }
        print(&*output, Pipe::Out, Line::new([(summary,).into()]).with_volume(Loudness::Shout));

        if app.dry_run {
            print(&*output, Pipe::Err, muted("Dry run: nothing was sent.".to_string()));
            return Ok(exit(failed + blocked));
        }

        // Pay.
        let mut sent = 0usize;
        let mut spent: Vec<Cost> = Vec::new();
        let mut incomplete = 0usize;
        if total_chars > 0 {
            if !self.yes {
                let prompt = format!(
                    "Send {} characters to {provider_name} for {} work(s)?",
                    number(total_chars),
                    works.len() - blocked
                );
                if !output.confirm(&prompt)? {
                    let hint = if output.is_interactive(Pipe::Err) {
                        "Nothing was sent."
                    } else {
                        "Nothing was sent. Use --yes to narrate without a prompt."
                    };
                    print(&*output, Pipe::Err, warning(hint.to_string()));
                    return Ok(ExitCode::FAILURE);
                }
            }

            let stop = CancellationToken::new();
            let narrating = Arc::new(AtomicBool::new(true));
            let signals = ctrl_c(stop.clone(), Arc::clone(&narrating), Arc::clone(&output));
            let bar = output.progress_bar("Narrating");
            bar.set_length(total_chars as u64);
            let mut stopped = false;
            for planned in &works {
                let options = NarrationOptions {
                    concurrency: self.concurrency,
                    max_chars: max_chars.map(|limit| limit.saturating_sub(sent)),
                    retry_mismatched: self.retry_mismatched,
                    approved: self.approve,
                    stop: Some(stop.clone()),
                    ..NarrationOptions::default()
                };
                let mut events = narrate(&planned.plan, Arc::clone(&provider), &store, options);
                while let Some(event) = futures::StreamExt::next(&mut events).await {
                    match event {
                        NarrationEvent::Started { .. } => {},
                        NarrationEvent::Recorded { chars, .. } => bar.inc(chars as u64),
                        NarrationEvent::Retrying { section, batch, attempt, delay, error } => print(
                            &*output,
                            Pipe::Err,
                            muted(format!(
                                "Work {}, section {section}, batch {}: {error}. Retry {attempt} in {} s.",
                                planned.work(),
                                batch + 1,
                                delay.as_secs()
                            )),
                        ),
                        NarrationEvent::Mismatched { section, batch, chars, .. } => {
                            bar.inc(chars as u64);
                            print(
                                &*output,
                                Pipe::Err,
                                warning(format!(
                                    "Work {}, section {section}, batch {}: the alignment does not match the text. The recording is stored, and --retry-mismatched pays for it again.",
                                    planned.work(),
                                    batch + 1
                                )),
                            );
                        },
                        NarrationEvent::SectionFailed { section, batch, error } => print(
                            &*output,
                            Pipe::Err,
                            danger(format!(
                                "Work {}, section {section}, batch {}: {error}. The other sections continue.",
                                planned.work(),
                                batch + 1
                            )),
                        ),
                        NarrationEvent::Stopped { reason } => {
                            if !matches!(reason, StopReason::NeedsApproval(_)) {
                                stopped = true;
                            }
                            let left = total_chars.saturating_sub(sent);
                            print(
                                &*output,
                                Pipe::Err,
                                danger(stop_message(&reason, planned, &provider_name, max_chars, left)),
                            );
                        },
                        NarrationEvent::Finished { chars, spent: costs } => {
                            sent += chars;
                            for cost in costs {
                                match spent.iter_mut().find(|total| total.unit == cost.unit) {
                                    Some(total) => total.amount += cost.amount,
                                    None => spent.push(cost),
                                }
                            }
                        },
                    }
                }
                if stopped {
                    break;
                }
            }
            bar.finish();
            narrating.store(false, Ordering::SeqCst);
            drop(signals);
        }

        // Export.
        let mut exported = 0usize;
        for planned in &works {
            let plan = match Plan::new(&planned.document, provider.as_ref(), &store).await {
                Ok(plan) => plan,
                Err(error) => {
                    incomplete += 1;
                    print(&*output, Pipe::Err, danger(format!("Work {}: {}", planned.work(), describe(error.frame()))));
                    continue;
                },
            };
            let total = plan.batches().count();
            let missing = total - plan.count(BatchState::Recorded);
            if missing > 0 {
                incomplete += 1;
                continue;
            }
            if self.no_export {
                continue;
            }
            save(&ctx, &store, &codec, &planned.version, &planned.document, &plan).await?;
            exported += 1;
            print(&*output, Pipe::Out, format_pair_line(Reason::Added, &planned.file, &planned.version, ctx.fandoms));
        }

        let mut summary = format!("Sent {} characters", number(sent));
        for cost in &spent {
            summary.push_str(&format!(" · spent {}", cost_text(cost)));
        }
        summary.push_str(&format!(" · exported {exported} audiobook(s)"));
        if incomplete > 0 {
            summary.push_str(&format!(" · {incomplete} work(s) not complete: run again to continue"));
        }
        print(&*output, Pipe::Out, Line::new([(summary,).into()]).with_volume(Loudness::Shout));
        Ok(exit(failed + incomplete))
    }
}

/// Listen for Ctrl-C. While narrating, the first one stops new requests (requests
/// in flight finish and are stored, because they are paid for); after that, or
/// outside narration, Ctrl-C quits at once. Dropping the guard stops listening.
fn ctrl_c(stop: CancellationToken, narrating: Arc<AtomicBool>, output: Arc<dyn Output>) -> AbortOnDrop {
    AbortOnDrop(tokio::spawn(async move {
        while tokio::signal::ctrl_c().await.is_ok() {
            if narrating.load(Ordering::SeqCst) && !stop.is_cancelled() {
                stop.cancel();
                print(
                    &*output,
                    Pipe::Err,
                    warning(
                        "Stopping after the requests in flight (they are paid for, so they are stored). Press Ctrl-C again to quit now."
                            .to_string(),
                    ),
                );
            } else {
                std::process::exit(130);
            }
        }
    }))
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn stop_message(
    reason: &StopReason,
    planned: &Planned,
    provider: &str,
    max_chars: Option<usize>,
    left: usize,
) -> String {
    match reason {
        StopReason::NeedsApproval(rating) => {
            let rated = match rating {
                Rating::Unrated => "is not rated".to_string(),
                other => format!("is rated {}", rating_name(*other)),
            };
            format!("Work {} {rated}. {provider} needs your approval for that rating: add --approve.", planned.work())
        },
        StopReason::Budget => format!(
            "Stopped at the {}-character limit. {} characters are left: run again to continue.",
            number(max_chars.unwrap_or_default()),
            number(left)
        ),
        StopReason::Provider(TtsErrorKind::QuotaExceeded) => format!("{provider} quota exceeded."),
        StopReason::Provider(TtsErrorKind::Unauthorised) => format!("{provider} refused the API key."),
        StopReason::Provider(error) => format!("{provider} stopped the run: {error}."),
        StopReason::Storage => "Recordings cannot be stored, so no more requests are sent.".to_string(),
        StopReason::Cancelled => "Stopped. The requests in flight were stored: run again to continue.".to_string(),
    }
}

fn rating_name(rating: Rating) -> &'static str {
    match rating {
        Rating::General => "General Audiences",
        Rating::Teen => "Teen And Up Audiences",
        Rating::Mature => "Mature",
        Rating::Explicit => "Explicit",
        Rating::Unrated => "Not Rated",
    }
}

fn cost_text(cost: &Cost) -> String {
    match cost.unit {
        CostUnit::Credits => format!("{} credits", number(cost.amount as usize)),
        CostUnit::Characters => format!("{} billed characters", number(cost.amount as usize)),
        CostUnit::Free => "nothing".to_string(),
    }
}

/// `31240` → `31,240`.
fn number(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn exit(problems: usize) -> ExitCode {
    if problems == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn print(output: &dyn Output, pipe: Pipe, line: Line<'_>) {
    output.print_to(pipe, &line);
}

fn muted(text: String) -> Line<'static> {
    Line::new([(text, &PALETTE.muted).into()])
}

fn warning(text: String) -> Line<'static> {
    Line::new([("WARN: ", &PALETTE.warning).into(), (text,).into()]).with_volume(Loudness::Shout)
}

fn danger(text: String) -> Line<'static> {
    Line::new([("\u{2717} ", &PALETTE.danger).into(), (text,).into()]).with_volume(Loudness::Shout)
}
