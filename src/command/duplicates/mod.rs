mod browser;
mod cluster;
mod content;

use self::browser::{Browser, Deletion, Outcome};
use self::cluster::{Cluster, build_clusters};
use super::Command;
use crate::context::{AppContext, BackendPurpose};
use crate::error::Result;
use crate::output::util::format_bytes;
use crate::output::{Line, Loudness, PALETTE, Piece, Pipe};
use clap::Args;
use rawr_storage::BackendHandle;
use rawr_storage::file::{FileInfo, Processed};
use std::collections::{BTreeSet, HashMap};
use std::process::ExitCode;

pub(crate) type File = FileInfo<Processed>;

/// Review near-duplicate versions: works downloaded more than once whose
/// content is semantically the same but differs by a few bytes of HTML or
/// whitespace, so each is tracked as a separate version.
///
/// Browse the clusters, diff their content side-by-side, and trash the ones you
/// deem equivalent. Trashing requires a configured `library.targets.trash`.
#[derive(Debug, Args)]
pub(crate) struct DuplicatesCommand {
    /// Restrict the review to a single AO3 work id.
    work_id: Option<u64>,
    /// Definitively detect markup/whitespace-only twins and diff the normalised
    /// prose instead of the raw HTML (costs extra CPU: parse + re-serialise).
    #[arg(short = 'n', long)]
    normalize: bool,
    /// Maximum word-count difference (percent) for two versions to be clustered.
    #[arg(long, default_value = "1.0")]
    word_tolerance: f64,
    /// Only consider versions present in this storage target.
    /// Defaults to the configured library target (`library.targets.import`).
    #[arg(long)]
    target: Option<String>,
    /// Print the cluster report and exit (no interactive browser, no deletion).
    #[arg(long)]
    list: bool,
}

impl Command for DuplicatesCommand {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode> {
        let target = self.target.as_deref().unwrap_or(ctx.config.library.targets.import.as_str());
        let mut clusters = build_clusters(&ctx.cache, self.work_id, target, self.word_tolerance).await?;

        if clusters.is_empty() {
            ctx.output.print(
                Pipe::Out,
                &Line::new([Piece::fixed(
                    format!("No near-duplicate versions found in target `{target}`."),
                    &PALETTE.muted,
                )])
                .with_volume(Loudness::Shout),
            );
            return Ok(ExitCode::SUCCESS);
        }

        let backends = resolve_backends(ctx, &clusters).await?;
        if self.normalize {
            content::normalize_pass(&mut clusters, &backends, ctx.output.as_ref()).await;
        }

        if self.list || !ctx.output.is_interactive(Pipe::Err) {
            report(ctx, &clusters, self.normalize);
            return Ok(ExitCode::SUCCESS);
        }

        let browser = Browser::new(clusters, backends, Some(&ctx.config.fandoms), self.normalize);
        match browser.run(ctx.output.as_ref()).await? {
            Outcome::Quit => Ok(ExitCode::SUCCESS),
            Outcome::Cleanup(deletions) => cleanup(ctx, deletions).await,
        }
    }
}

/// Resolve a storage backend for every target referenced by the clustered files.
///
/// The cache may hold file records under target names that are no longer
/// defined in the config (e.g. a target was renamed). Those are skipped with a
/// warning rather than failing the whole review.
async fn resolve_backends(ctx: &AppContext, clusters: &[Cluster]) -> Result<HashMap<String, BackendHandle>> {
    let names: BTreeSet<&str> = clusters
        .iter()
        .flat_map(|c| c.candidates.iter())
        .flat_map(|c| c.files.iter())
        .map(|f| f.target.as_str())
        .collect();
    let mut backends = HashMap::new();
    for name in names {
        if !ctx.config.targets.contains_key(name) {
            ctx.output.print(
                Pipe::Err,
                &Line::new([
                    ("WARN: ", &PALETTE.warning).into(),
                    (format!("ignoring files recorded under target `{name}` (not defined in config)"),).into(),
                ])
                .with_volume(Loudness::Shout),
            );
            continue;
        }
        backends.insert(name.to_string(), ctx.get_backend_by_name(name).await?);
    }
    Ok(backends)
}

/// Print a non-interactive cluster report (used by `--list` and non-terminals).
fn report(ctx: &AppContext, clusters: &[Cluster], normalize: bool) {
    let output = ctx.output.as_ref();
    let total: usize = clusters.iter().map(|c| c.candidates.len()).sum();
    output.print(
        Pipe::Out,
        &Line::new([
            Piece::fixed(format!("{} near-duplicate cluster(s)", clusters.len()), &PALETTE.heading),
            Piece::fixed(format!(" \u{00b7} {total} versions"), &PALETTE.muted),
        ])
        .with_volume(Loudness::Shout),
    );
    output.print(Pipe::Out, &Line::empty().with_volume(Loudness::Shout));

    for cluster in clusters {
        output.print(
            Pipe::Out,
            &Line::new([
                Piece::fixed(format!("#{}", cluster.work_id), &PALETTE.highlight),
                Piece::space(),
                Piece::flex(cluster.title.as_str(), &PALETTE.success, 16),
                Piece::fixed(format!("  ({} versions)", cluster.candidates.len()), &PALETTE.muted),
            ])
            .with_volume(Loudness::Shout),
        );
        for (i, candidate) in cluster.candidates.iter().enumerate() {
            let version = &candidate.version;
            let mut pieces = vec![
                Piece::fixed(if i == 0 { "  \u{2605} " } else { "    " }, &PALETTE.success),
                Piece::fixed(format!("{:08x}", version.crc32), &PALETTE.accent),
                Piece::space(),
                Piece::fixed(format!("{:.1}k", version.metadata.words.max(101) as f32 / 1000.0), &PALETTE.highlight),
                Piece::space(),
                Piece::fixed(version.metadata.chapters.to_string(), &PALETTE.muted),
                Piece::space(),
                Piece::fixed(version.metadata.last_modified.to_string(), &PALETTE.muted),
                Piece::space(),
                Piece::fixed(format_bytes(version.length), &PALETTE.muted),
            ];
            if i == 0 {
                pieces.push(Piece::fixed("  recommended", &PALETTE.success));
            } else if normalize && candidate.identical {
                pieces.push(Piece::fixed("  \u{2261} identical", &PALETTE.success));
            } else if normalize && let Some(ratio) = candidate.similarity {
                pieces.push(Piece::fixed(format!("  ~{:.1}% similar", ratio * 100.0), &PALETTE.warning));
            }
            output.print(Pipe::Out, &Line::new(pieces).with_volume(Loudness::Shout));
        }
        output.print(Pipe::Out, &Line::empty().with_volume(Loudness::Shout));
    }
}

/// Trash every marked version's files (across all targets) then drop its cache
/// rows. Refuses to act unless a trash target is configured. Dry-run safe.
async fn cleanup(ctx: &AppContext, deletions: Vec<Deletion>) -> Result<ExitCode> {
    if deletions.is_empty() {
        ctx.output.print(
            Pipe::Out,
            &Line::new([Piece::fixed("Nothing marked for cleanup.", &PALETTE.muted)]).with_volume(Loudness::Shout),
        );
        return Ok(ExitCode::SUCCESS);
    }

    let Some(trash) = ctx.get_backend_by_purpose(BackendPurpose::Trash).await? else {
        ctx.output.print(
            Pipe::Err,
            &Line::new([
                Piece::fixed("\u{2717} ", &PALETTE.danger),
                Piece::fixed(
                    "Cleanup needs a trash target. Configure one under `library.targets.trash` in your config.",
                    &PALETTE.danger,
                ),
            ])
            .with_volume(Loudness::Shout),
        );
        return Ok(ExitCode::FAILURE);
    };

    let file_count: usize = deletions.iter().map(|d| d.files.len()).sum();
    if !ctx.dry_run
        && !ctx.output.confirm(&format!("Trash {} version(s) ({} file(s))?", deletions.len(), file_count))?
    {
        ctx.output.print(
            Pipe::Out,
            &Line::new([Piece::fixed("Cancelled.", &PALETTE.muted)]).with_volume(Loudness::Shout),
        );
        return Ok(ExitCode::SUCCESS);
    }

    let bar = ctx.output.progress_bar("Trashing");
    bar.set_length(deletions.len() as u64);
    for deletion in &deletions {
        for file in &deletion.files {
            if !ctx.config.targets.contains_key(file.target.as_str()) {
                ctx.output.print(
                    Pipe::Err,
                    &Line::new([
                        ("WARN: ", &PALETTE.warning).into(),
                        (format!("skipping `{}` in target `{}` (not defined in config)", file.path, file.target),)
                            .into(),
                    ])
                    .with_volume(Loudness::Shout),
                );
                continue;
            }
            let backend = ctx.get_backend_by_name(&file.target).await?;
            rawr_library::trash(&backend, &trash, file).await?;
        }
        ctx.cache.delete_by_content_hash(&deletion.content_hash).await?;
        ctx.output.print(
            Pipe::Out,
            &Line::new([
                Piece::fixed("\u{00d7} ", &PALETTE.danger),
                Piece::fixed(format!("trashed {}", deletion.label), &PALETTE.danger),
            ])
            .with_volume(Loudness::Shout),
        );
        bar.inc(1);
    }
    bar.finish_and_clear();

    let verb = if ctx.dry_run { "Would trash" } else { "Trashed" };
    ctx.output.print(
        Pipe::Out,
        &Line::new([Piece::fixed(format!("{verb} {} version(s).", deletions.len()), &PALETTE.success)])
            .with_volume(Loudness::Shout),
    );
    Ok(ExitCode::SUCCESS)
}
