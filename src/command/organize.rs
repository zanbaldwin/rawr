use super::Command;
use super::color;
use crate::command::TerminalText;
use crate::command::TextPiece;
use crate::context::{AppContext, BackendPurpose};
use crate::error::Result;
use clap::Args;
use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rawr_compress::cli::Preference;
use rawr_library::Context as LibraryContext;
use rawr_library::organize::{Action, OrganizeEvent, organize};
use similar::{ChangeTag, TextDiff};
use std::pin::pin;
use std::process::ExitCode;
use std::sync::Arc;

#[derive(Debug, Args)]
pub(crate) struct OrganizeCommand {
    /// Compress files during organize. Optionally specify format (eg, --compress=bz2).
    /// Without a value, uses the configured default. Omit to preserve existing compression.
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

        let lib_ctx = Arc::new(LibraryContext::new(
            ctx.config.library.path_templates.import.parse()?,
            compression,
            ctx.get_backend_by_purpose(BackendPurpose::Trash).await?,
        ));
        let mut stream = pin!(organize(&import_backend, &ctx.cache, lib_ctx));

        let multi = MultiProgress::new();
        let bar = multi.add(ProgressBar::new(0));
        bar.enable_steady_tick(std::time::Duration::from_millis(100));
        bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} Organizing [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) [{elapsed_precise}]",
                )
                .expect("valid template"),
        );

        let mut error_count: u64 = 0;

        while let Some(event) = stream.next().await {
            match event {
                Ok(OrganizeEvent::Started) => {},
                Ok(OrganizeEvent::DiscoveryComplete(total)) => {
                    bar.set_length(total);
                },
                Ok(OrganizeEvent::Organized(action)) => {
                    // TODO: Would be nice to format_processed() the version too,
                    // then the diff on the second line. But would that make it
                    // too crowded?
                    bar.println(format_action(&action, ctx.use_colour));
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

        multi.clear().ok();

        if error_count > 0 {
            tracing::warn!("{error_count} file(s) failed during organize");
        }
        Ok(ExitCode::SUCCESS)
    }
}

fn tokenize_path(path: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = 0;
    for (i, c) in path.char_indices() {
        if matches!(c, '/' | '-' | '.' | '_' | ' ' | '\\') {
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

fn format_action(action: &Action, use_colour: bool) -> String {
    match action {
        Action::Renamed { from, to } => {
            if !use_colour {
                return format!("· {from} -> {to}");
            }
            let old_tokens = tokenize_path(from);
            let new_tokens = tokenize_path(to);
            let mut line = vec![TextPiece::new("· ", None)];
            line.extend(TextDiff::from_slices(&old_tokens, &new_tokens).iter_all_changes().map(|c| {
                TextPiece::new(
                    c.value(),
                    Some(match c.tag() {
                        ChangeTag::Equal => color::GRAY,
                        ChangeTag::Delete => color::RED,
                        ChangeTag::Insert => color::GREEN,
                    }),
                )
            }));
            line.display(use_colour).into()
        },
        Action::AlreadyCorrect(p) => TextPiece::new(format!("= {p}"), Some(color::GRAY)).display(use_colour).into(),
        Action::CleanedUp(p) => TextPiece::new(format!("× {p}"), Some(color::RED)).display(use_colour).into(),
    }
}
