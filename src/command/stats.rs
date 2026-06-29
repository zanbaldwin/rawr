use super::Command;
use crate::context::AppContext;
use crate::error::Result;
use crate::output::{format_bytes, format_number};
use clap::Args;
use console::Style;
use rawr_config::models::FandomConfig;
use rawr_output::{Line, Loudness, PALETTE, Piece, Pipe};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::process::ExitCode;
use time::UtcDateTime;

#[derive(Debug, Args)]
pub(crate) struct StatsCommand {
    /// Number of top items to show for fandoms, characters, ships, and tags
    #[arg(long, default_value = "3")]
    top: u8,
}

fn middot<'a>() -> Piece<'a> {
    Piece::fixed(" \u{00b7} ", &PALETTE.muted)
}

fn label(text: &str, width: usize) -> String {
    format!("  {text:<width$}")
}

fn format_date(dt: UtcDateTime) -> String {
    let date = dt.date();
    format!("{} {}", date.month(), date.year())
}

fn merge_fandom_aliases(all: Vec<(String, u64)>, config: &FandomConfig, top: u8) -> (Vec<(String, u64)>, u64) {
    let mut merged: HashMap<String, u64> = HashMap::new();
    for (name, count) in all {
        let canonical = config.display_name(&name);
        if canonical == name {
            *merged.entry(name).or_default() += count;
        } else {
            *merged.entry(canonical.to_string()).or_default() += count;
        }
    }
    let count = merged.len() as u64;
    let mut result: Vec<_> = merged.into_iter().collect();
    result.sort_unstable_by_key(|(_, c)| Reverse(*c));
    result.truncate(top as usize);
    (result, count)
}

fn pieces_with_counts<'a>(items: &'a [(String, u64)], style: &'a Style) -> Vec<Piece<'a>> {
    let mut pieces = Vec::new();
    for (i, (name, count)) in items.iter().enumerate() {
        if i > 0 {
            pieces.push(middot());
        }
        pieces.push(Piece::plain(format_number(*count)));
        pieces.push(Piece::plain("\u{00d7} "));
        pieces.push(Piece::fixed(name, style));
    }
    pieces
}

fn pieces_key_value<'a>(items: impl Iterator<Item = (&'a str, u64)>, style: &'a Style) -> Vec<Piece<'a>> {
    let mut pieces = Vec::new();
    for (i, (key, count)) in items.enumerate() {
        if i > 0 {
            pieces.push(middot());
        }
        pieces.push(Piece::plain(format!("{key}: ")));
        pieces.push(Piece::fixed(format_number(count), style));
    }
    pieces
}

fn row_with_counts<'a>(
    lbl: &str,
    w: usize,
    items: &'a [(String, u64)],
    style: &'a Style,
    unique_count: Option<u64>,
) -> Vec<Piece<'a>> {
    let mut pieces = vec![Piece::fixed(label(lbl, w), &PALETTE.label)];
    if let Some(count) = unique_count {
        pieces.push(Piece::fixed(format!("{} unique", format_number(count)), &PALETTE.muted));
        pieces.push(middot());
    }
    pieces.extend(pieces_with_counts(items, style));
    pieces
}

impl Command for StatsCommand {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode> {
        let target = &ctx.config.library.targets.import;
        let stats = ctx.cache.stats(target, self.top).await?;

        if stats.works == 0 {
            ctx.output.print(
                Pipe::Out,
                &Line::new([Piece::fixed(
                    "Your library is empty. Run `rawr scan` or `rawr import` to get started.",
                    &PALETTE.muted,
                )])
                .with_volume(Loudness::Shout),
            );
            return Ok(ExitCode::SUCCESS);
        }

        // Resolve fandoms: merge aliases if config has renames, otherwise take top-N directly
        let raw_fandom_count = stats.fandoms.len() as u64;
        let (display_fandoms, fandom_count, fandom_unique) = if !ctx.config.fandoms.renames.is_empty() {
            let (merged, count) = merge_fandom_aliases(stats.fandoms, &ctx.config.fandoms, self.top);
            (merged, count, Some(raw_fandom_count))
        } else {
            let top: Vec<_> = stats.fandoms.into_iter().take(self.top as usize).collect();
            (top, raw_fandom_count, None)
        };

        // Pre-compute dynamic labels for section 3
        let fandom_label = format!("{} fandoms", format_number(fandom_count));

        let tags_label = format!("{} tags", format_number(stats.tag_count));
        let series_label = format!("{} series", format_number(stats.series_count));

        // Compute label width as max(all labels) + 2
        let all_labels = [
            "Words",
            "Storage",
            "Completion",
            "Ratings",
            "Date range",
            "Languages",
            &fandom_label,
            "Characters",
            "Top ships",
            &tags_label,
            &series_label,
        ];
        let w = all_labels.iter().map(|l| l.len()).max().unwrap_or(0) + 2;

        // Section 1: Headline
        ctx.output.print(
            Pipe::Out,
            &Line::new([
                Piece::fixed("Your library:", &PALETTE.heading),
                Piece::space(),
                Piece::fixed(format_number(stats.works), &PALETTE.highlight),
                Piece::plain(" works across "),
                Piece::fixed(format_number(stats.versions), &PALETTE.highlight),
                Piece::plain(" versions in "),
                Piece::fixed(format_number(stats.files), &PALETTE.highlight),
                Piece::plain(" files."),
            ])
            .with_volume(Loudness::Shout),
        );
        ctx.output.print(Pipe::Out, &Line::empty().with_volume(Loudness::Shout));

        // Section 2: Key metrics
        ctx.output.print(
            Pipe::Out,
            &Line::new([
                Piece::fixed(label("Words", w), &PALETTE.label),
                Piece::fixed(format_number(stats.total_words_best_versions), &PALETTE.highlight),
                Piece::plain(" across works"),
                middot(),
                Piece::fixed(format_number(stats.total_words_all_versions), &PALETTE.highlight),
                Piece::plain(" across all versions"),
            ])
            .with_volume(Loudness::Shout),
        );

        let ratio = if stats.total_file_size > 0 {
            format!(" ({:.1}\u{00d7} ratio)", stats.total_content_size as f64 / stats.total_file_size as f64)
        } else {
            String::new()
        };
        ctx.output.print(
            Pipe::Out,
            &Line::new([
                Piece::fixed(label("Storage", w), &PALETTE.label),
                Piece::fixed(format_bytes(stats.total_file_size), &PALETTE.highlight),
                Piece::plain(" compressed"),
                middot(),
                Piece::fixed(format_bytes(stats.total_content_size), &PALETTE.highlight),
                Piece::plain(" uncompressed"),
                Piece::fixed(ratio, &PALETTE.muted),
            ])
            .with_volume(Loudness::Shout),
        );

        ctx.output.print(
            Pipe::Out,
            &Line::new([
                Piece::fixed(label("Completion", w), &PALETTE.label),
                Piece::fixed(format_number(stats.complete_works), &PALETTE.success),
                Piece::plain(" complete"),
                middot(),
                Piece::fixed(format_number(stats.incomplete_works), &PALETTE.warning),
                Piece::plain(" in progress"),
            ])
            .with_volume(Loudness::Shout),
        );

        if !stats.ratings.is_empty() {
            let items = stats.ratings.iter().map(|(r, c)| (r.as_deref().unwrap_or("NR"), *c));
            let mut pieces = vec![Piece::fixed(label("Ratings", w), &PALETTE.label)];
            pieces.extend(pieces_key_value(items, &PALETTE.highlight));
            ctx.output.print(Pipe::Out, &Line::new(pieces).with_volume(Loudness::Shout));
        }

        if let (Some(oldest), Some(newest)) = (stats.oldest_published, stats.newest_published) {
            ctx.output.print(
                Pipe::Out,
                &Line::new([
                    Piece::fixed(label("Date range", w), &PALETTE.label),
                    Piece::fixed(format_date(oldest), &PALETTE.highlight),
                    Piece::plain(" \u{2014} "),
                    Piece::fixed(format_date(newest), &PALETTE.highlight),
                ])
                .with_volume(Loudness::Shout),
            );
        }

        if !stats.languages.is_empty() {
            let mut pieces = vec![Piece::fixed(label("Languages", w), &PALETTE.label)];
            pieces.push(Piece::fixed(stats.languages.len().to_string(), &PALETTE.highlight));
            pieces.push(Piece::plain(" ("));
            let items = stats.languages.iter().take(self.top as usize).map(|(l, c)| (l.as_str(), *c));
            pieces.extend(pieces_key_value(items, &PALETTE.highlight));
            pieces.push(Piece::plain(")"));
            ctx.output.print(Pipe::Out, &Line::new(pieces).with_volume(Loudness::Shout));
        }

        // Section 3: Top tags
        ctx.output.print(Pipe::Out, &Line::empty().with_volume(Loudness::Shout));

        if !display_fandoms.is_empty() {
            let pieces = row_with_counts(&fandom_label, w, &display_fandoms, &PALETTE.warning, fandom_unique);
            ctx.output.print(Pipe::Out, &Line::new(pieces).with_volume(Loudness::Shout));
        }

        if !stats.top_characters.is_empty() {
            let pieces = row_with_counts("Characters", w, &stats.top_characters, &PALETTE.success, None);
            ctx.output.print(Pipe::Out, &Line::new(pieces).with_volume(Loudness::Shout));
        }

        if !stats.top_relationships.is_empty() {
            let pieces = row_with_counts("Top ships", w, &stats.top_relationships, &PALETTE.accent, None);
            ctx.output.print(Pipe::Out, &Line::new(pieces).with_volume(Loudness::Shout));
        }

        if !stats.top_freeform_tags.is_empty() {
            let pieces = row_with_counts(
                &tags_label,
                w,
                &stats.top_freeform_tags,
                &PALETTE.success,
                Some(stats.unique_tag_count),
            );
            ctx.output.print(Pipe::Out, &Line::new(pieces).with_volume(Loudness::Shout));
        }

        if stats.series_count > 0 {
            let pieces = [
                Piece::fixed(label(&series_label, w), &PALETTE.label),
                Piece::plain(format!("containing {} works", format_number(stats.works_in_series))),
            ];
            ctx.output.print(Pipe::Out, &Line::new(pieces).with_volume(Loudness::Shout));
        }

        // Section 4: Sign-off
        ctx.output.print(Pipe::Out, &Line::empty().with_volume(Loudness::Shout));
        ctx.output.print(
            Pipe::Out,
            &Line::new([Piece::fixed("Happy reading!", &PALETTE.muted)]).with_volume(Loudness::Shout),
        );

        Ok(ExitCode::SUCCESS)
    }
}
