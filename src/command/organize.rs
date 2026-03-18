use super::Command;
use crate::context::{AppContext, BackendPurpose};
use crate::error::Result;
use crate::output::{Line, Loudness, PALETTE, Piece, Pipe};
use clap::Args;
use futures::StreamExt;
use rawr_compress::cli::Preference;
use rawr_library::organize::{Action, OrganizeEvent, organize};
use similar::{ChangeTag, TextDiff};
use std::pin::pin;
use std::process::ExitCode;

/// Reorganize files in the import target to match the configured path template.
///
/// Scans all files in the import backend, computes each file's expected
/// path from its metadata and the configured template, and renames any
/// mismatched files. Files already at their correct path are skipped.
#[derive(Debug, Args)]
pub(crate) struct OrganizeCommand {
    /// Compress files during organize. Optionally specify format (eg, --compress=bz2).
    /// Without a value, uses the configured default. Omit to preserve existing compression.
    ///
    /// Formats: none, gz (gzip), bz2 (bzip2), br (brotli), bz3 (bzip3), xz (lzma), zst (zstd).
    /// Some formats require the corresponding feature flag at compile time.
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    compress: Option<String>,
}
impl Command for OrganizeCommand {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode> {
        // Clap can't handle `Option<Option<String>>` so we have to manually
        // map the configured default in place of an empty string.
        let flag: rawr_compress::cli::Flag = self.compress.clone().map(|s| if s.is_empty() { None } else { Some(s) });
        let compression = match Preference::try_from(flag)? {
            Preference::Explicit(c) => Some(c),
            Preference::Implicit => Some(ctx.config.library.compression),
            Preference::NotSpecified => None,
        };

        let import_backend = ctx.get_backend_by_purpose(BackendPurpose::Import).await?.ok_or_else(|| {
            miette::miette!(
                "No import target configured. Define one in your config file under `library.targets.import`."
            )
        })?;

        let lib_ctx = ctx.get_library_context(compression).await?;
        let mut stream = pin!(organize(&import_backend, &ctx.cache, lib_ctx));
        let bar = ctx.output.progress_bar("Organizing");

        let mut error_count: u64 = 0;
        while let Some(event) = stream.next().await {
            match event {
                Ok(OrganizeEvent::Started) => {},
                Ok(OrganizeEvent::DiscoveryComplete(total)) => {
                    bar.set_length(total);
                },
                Ok(OrganizeEvent::Organized(action)) => {
                    let line: Line = format_action(&action).into();
                    let loudness = match &action {
                        Action::Renamed { .. } => Loudness::Normal,
                        Action::AlreadyCorrect(_) => Loudness::Whisper,
                        Action::CleanedUp(_) => Loudness::Shout,
                    };
                    ctx.output.print(Pipe::Out, &line.with_volume(loudness));
                    bar.inc(1);
                },
                Ok(OrganizeEvent::Complete) => {
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
                    ("WARN: ", &PALETTE.warning).into(),
                    (format!("{error_count} file(s) failed during organize"),).into(),
                ])
                .with_volume(Loudness::Shout),
            );
        }
        Ok(ExitCode::SUCCESS)
    }
}

const TOKEN_BOUNDARIES: [char; 6] = ['/', '-', '.', '_', ' ', '\\'];
fn tokenize_path(path: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = 0;
    for (i, c) in path.char_indices() {
        if TOKEN_BOUNDARIES.contains(&c) {
            if start < i {
                tokens.push(&path[start..i]);
            }
            tokens.push(&path[i..i + c.len_utf8()]);
            start = i + c.len_utf8();
        }
    }
    if start < path.len() {
        tokens.push(&path[start..]);
    }
    tokens
}

fn format_action<'a>(action: &'a Action) -> Vec<Piece<'a>> {
    match action {
        Action::Renamed { from, to } => {
            let old_tokens = tokenize_path(from);
            let new_tokens = tokenize_path(to);
            let mut pieces: Vec<Piece<'_>> = vec![("·", &PALETTE.muted).into(), Piece::space()];
            for change in TextDiff::from_slices(&old_tokens, &new_tokens).iter_all_changes() {
                let style = match change.tag() {
                    ChangeTag::Equal => &PALETTE.muted,
                    ChangeTag::Delete => &PALETTE.removed,
                    ChangeTag::Insert => &PALETTE.added,
                };
                pieces.push(Piece::fixed(change.value().to_string(), style));
            }
            pieces
        },
        Action::AlreadyCorrect(p) => {
            vec![
                ("=", &PALETTE.muted).into(),
                Piece::space(),
                (p.as_str(), &PALETTE.muted).into(),
            ]
        },
        Action::CleanedUp(p) => {
            vec![
                ("×", &PALETTE.danger).into(),
                Piece::space(),
                (p.as_str(), &PALETTE.danger).into(),
            ]
        },
    }
}
