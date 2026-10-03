use super::Command;
use super::export::ExportContext;
use super::narrate::{danger, muted, number, print, warning};
use crate::context::AppContext;
use crate::error::Result;
use clap::{ArgGroup, Args, Subcommand};
use rawr_app::audio::recordings;
use rawr_app::works::WorkRef;
use rawr_audiobook::store::{BatchKey, CostUnitMeta, Ledger, Store};
use rawr_config::models::StyleFormat;
use rawr_output::{Line, Loudness, Pipe};
use std::collections::BTreeSet;
use std::process::ExitCode;

const AO3_WORK_PREFIX: &str = "https://archiveofourown.org/works/";

/// Show or delete paid narration recordings.
#[derive(Debug, Args)]
pub(crate) struct RecordingsCommand {
    #[command(subcommand)]
    action: RecordingsAction,
}

#[derive(Debug, Subcommand)]
enum RecordingsAction {
    /// List narrated works: recordings, credits spent, and whether the audiobook exists
    List,
    /// Delete paid recordings (asks first; this cannot be undone)
    Clear(ClearArgs),
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("scope").required(true).multiple(false)))]
struct ClearArgs {
    /// Works whose recordings to delete. Accepts work IDs, work_id@hash, or file paths.
    #[arg(group = "scope")]
    works: Vec<WorkRef>,
    /// Delete the recordings of every work whose audiobook exists in the audio target.
    #[arg(long, group = "scope")]
    exported: bool,
    /// Delete the recordings of every work. Always asks, even with --yes.
    #[arg(long, group = "scope")]
    all: bool,
    /// Delete recordings that no work uses.
    #[arg(long, group = "scope")]
    orphans: bool,
    /// Do not ask for confirmation (except with --all).
    #[arg(short = 'y', long)]
    yes: bool,
}

/// What the store knows about one narrated work.
struct Entry {
    ledger: Ledger,
    recorded: usize,
    credits: u64,
    exported: Option<bool>,
}

impl Command for RecordingsCommand {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode> {
        let app: &AppContext = ctx;
        let ctx = ExportContext::try_from_app(app, StyleFormat::Audiobook).await?;
        let store = recordings(&app.config, app.dry_run)?;
        match &self.action {
            RecordingsAction::List => list(&ctx, &store).await,
            RecordingsAction::Clear(args) => clear(app, &ctx, &store, args).await,
        }
    }
}

async fn list(ctx: &ExportContext<'_>, store: &Store) -> Result<ExitCode> {
    let entries = entries(ctx, store).await?;
    if entries.is_empty() {
        print(&*ctx.output, Pipe::Err, muted("No recordings.".to_string()));
    }
    for entry in &entries {
        let total = entry.ledger.keys().count();
        let mut text = format!(
            "{} \"{}\" · {}/{} recorded · {} credits · planned {}",
            label(&entry.ledger),
            entry.ledger.title,
            entry.recorded,
            total,
            number(entry.credits as usize),
            entry.ledger.planned_at.date(),
        );
        match entry.exported {
            Some(true) => text.push_str(" · exported"),
            Some(false) => text.push_str(" · not exported"),
            None => {},
        }
        print(&*ctx.output, Pipe::Out, Line::new([(text,).into()]));
    }
    let orphans = store.orphans().await?;
    if !orphans.is_empty() {
        print(
            &*ctx.output,
            Pipe::Out,
            muted(format!(
                "{} recording(s) used by no work: `rawr recordings clear --orphans` deletes them",
                orphans.len()
            )),
        );
    }
    Ok(ExitCode::SUCCESS)
}

async fn clear(app: &AppContext, ctx: &ExportContext<'_>, store: &Store, args: &ClearArgs) -> Result<ExitCode> {
    let output = &*ctx.output;
    if args.orphans {
        let orphans = store.orphans().await?;
        if orphans.is_empty() {
            print(output, Pipe::Err, muted("No orphan recordings.".to_string()));
            return Ok(ExitCode::SUCCESS);
        }
        let credits = credits(store, &orphans).await?;
        let prompt = format!(
            "Delete {} orphan recording(s) ({} credits)? This cannot be undone.",
            orphans.len(),
            number(credits as usize)
        );
        if !confirmed(app, ctx, &prompt, args.yes)? {
            return Ok(if app.dry_run { ExitCode::SUCCESS } else { ExitCode::FAILURE });
        }
        let deleted = store.delete_orphans().await?;
        print(output, Pipe::Out, Line::new([(format!("Deleted {} recording(s).", deleted.len()),).into()]));
        return Ok(ExitCode::SUCCESS);
    }

    let entries = entries(ctx, store).await?;
    let mut problems = 0;
    let chosen: Vec<&Entry> = if args.all {
        entries.iter().collect()
    } else if args.exported {
        entries.iter().filter(|entry| entry.exported == Some(true)).collect()
    } else {
        let mut chosen = Vec::new();
        for work in &args.works {
            let identifier = match work {
                WorkRef::BestWork(id) | WorkRef::WorkVersion(id, _) => format!("{AO3_WORK_PREFIX}{id}"),
                WorkRef::FilePath(_) => match rawr_app::works::resolve(&ctx.cache, ctx.load.name(), work).await {
                    Ok((version, _)) => format!("{AO3_WORK_PREFIX}{}", version.metadata.work_id),
                    Err(error) => {
                        problems += 1;
                        print(output, Pipe::Err, danger(error.to_string()));
                        continue;
                    },
                },
            };
            match entries.iter().find(|entry| entry.ledger.identifier == identifier) {
                Some(entry) => chosen.push(entry),
                None => {
                    problems += 1;
                    print(output, Pipe::Err, danger(format!("{identifier} has no recordings")));
                },
            }
        }
        chosen
    };
    if chosen.is_empty() {
        print(output, Pipe::Err, muted("Nothing to delete.".to_string()));
        return Ok(if problems == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE });
    }

    // Recordings that another work still uses are kept (as `Store::delete_document` does).
    let chosen_ids: BTreeSet<&str> = chosen.iter().map(|entry| entry.ledger.identifier.as_str()).collect();
    let kept: BTreeSet<BatchKey> = entries
        .iter()
        .filter(|entry| !chosen_ids.contains(entry.ledger.identifier.as_str()))
        .flat_map(|entry| entry.ledger.keys().collect::<Vec<_>>())
        .collect();
    let doomed: BTreeSet<BatchKey> = chosen
        .iter()
        .flat_map(|entry| entry.ledger.keys().collect::<Vec<_>>())
        .filter(|key| !kept.contains(key))
        .collect();
    let doomed: Vec<BatchKey> = doomed.into_iter().collect();
    let credits = credits(store, &doomed).await?;
    for entry in &chosen {
        print(
            output,
            Pipe::Out,
            Line::new([(format!("{} \"{}\"", label(&entry.ledger), entry.ledger.title),).into()]),
        );
    }
    let prompt = format!(
        "Delete {} recording(s) ({} credits) of {} work(s)? This cannot be undone.",
        doomed.len(),
        number(credits as usize),
        chosen.len()
    );
    if !confirmed(app, ctx, &prompt, args.yes && !args.all)? {
        return Ok(if app.dry_run { ExitCode::SUCCESS } else { ExitCode::FAILURE });
    }
    let mut deleted = 0;
    for entry in &chosen {
        deleted += store.delete_document(&entry.ledger.identifier).await?.len();
    }
    print(
        output,
        Pipe::Out,
        Line::new([(format!("Deleted {deleted} recording(s)."),).into()]).with_volume(Loudness::Shout),
    );
    Ok(if problems == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// Ask before deleting. Dry-run never deletes, and without a terminal only `--yes` deletes.
fn confirmed(app: &AppContext, ctx: &ExportContext<'_>, prompt: &str, yes: bool) -> Result<bool> {
    let output = &*ctx.output;
    if app.dry_run {
        print(output, Pipe::Err, muted(format!("Dry run: {prompt} No.")));
        return Ok(false);
    }
    if yes || output.confirm(prompt)? {
        return Ok(true);
    }
    let hint = if output.is_interactive(Pipe::Err) {
        "Nothing was deleted."
    } else {
        "Nothing was deleted. Use --yes to delete without a prompt (--all always asks)."
    };
    print(output, Pipe::Err, warning(hint.to_string()));
    Ok(false)
}

async fn entries(ctx: &ExportContext<'_>, store: &Store) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for ledger in store.ledgers().await? {
        let keys: Vec<BatchKey> = ledger.keys().collect();
        let mut recorded = 0;
        for key in &keys {
            if store.exists(key).await? {
                recorded += 1;
            }
        }
        let credits = credits(store, &keys).await?;
        let exported = exported(ctx, &ledger).await;
        entries.push(Entry { ledger, recorded, credits, exported });
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.ledger.planned_at));
    Ok(entries)
}

/// Whether the audiobook of the ledger's version exists in the audio target.
/// `None` when that cannot be told (not an AO3 work, or the version is not in the library).
async fn exported(ctx: &ExportContext<'_>, ledger: &Ledger) -> Option<bool> {
    let work: u64 = ledger.identifier.strip_prefix(AO3_WORK_PREFIX)?.parse().ok()?;
    let hash = ledger.source_hash.as_deref()?;
    let (version, _) =
        ctx.cache.get_by_work_id(work).await.ok()?.into_iter().find(|(version, _)| version.hash == hash)?;
    let path = ctx.path_generator.generate(&version, "epub", None).ok()?;
    ctx.save.exists(&path).await.ok()
}

/// Credits paid for these recordings, from their receipts.
async fn credits(store: &Store, keys: &[BatchKey]) -> Result<u64> {
    let mut total = 0;
    for key in keys {
        if let Some(cost) = store.meta(key).await?.and_then(|meta| meta.receipt.cost)
            && matches!(cost.unit, CostUnitMeta::Credits)
        {
            total += cost.amount;
        }
    }
    Ok(total)
}

fn label(ledger: &Ledger) -> String {
    match ledger.identifier.strip_prefix(AO3_WORK_PREFIX) {
        Some(work) => format!("#{work}"),
        None => ledger.identifier.clone(),
    }
}
