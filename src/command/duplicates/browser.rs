use super::File;
use super::cluster::{Candidate, Cluster, Mark};
use super::content::{compute_diff, read_member_text};
use crate::error::{Error, Result};
use crate::output::util::format_bytes;
use crate::output::{Line, Loudness, Output, PALETTE, Piece, Pipe};
use console::{Key, Term};
use rawr_config::models::FandomConfig;
use rawr_storage::BackendHandle;
use std::collections::HashMap;

const PIPE: Pipe = Pipe::Err;

fn term_err(e: std::io::Error) -> Error {
    miette::miette!("Terminal error: {e}").into()
}

/// A flattened reference to a visible row in the cluster list.
#[derive(Clone, Copy)]
enum Row {
    Header(usize),
    Member(usize, usize),
}

enum Action {
    Continue,
    OpenDiff(usize, usize),
    Cleanup,
    Quit,
}

/// The user's decision when leaving the browser.
pub(crate) enum Outcome {
    Quit,
    Cleanup(Vec<Deletion>),
}

/// A version the user marked for cleanup, with every file referencing it.
pub(crate) struct Deletion {
    pub content_hash: String,
    pub files: Vec<File>,
    pub label: String,
}

/// Interactive cluster browser: navigate near-duplicate clusters, view
/// side-by-side diffs, and mark versions to trash.
pub(crate) struct Browser<'a> {
    clusters: Vec<Cluster>,
    backends: HashMap<String, BackendHandle>,
    fandoms: Option<&'a FandomConfig>,
    normalize: bool,
    cursor: usize,
    scroll_offset: usize,
}
impl<'a> Browser<'a> {
    pub(crate) fn new(
        clusters: Vec<Cluster>,
        backends: HashMap<String, BackendHandle>,
        fandoms: Option<&'a FandomConfig>,
        normalize: bool,
    ) -> Self {
        Self { clusters, backends, fandoms, normalize, cursor: 0, scroll_offset: 0 }
    }

    pub(crate) async fn run(mut self, output: &dyn Output) -> Result<Outcome> {
        if !output.is_interactive(PIPE) {
            return Ok(Outcome::Quit);
        }
        let (_guard, term) = output.alt(PIPE)?;
        let mut lines_drawn: usize = 0;
        loop {
            if lines_drawn > 0 {
                term.clear_last_lines(lines_drawn).map_err(term_err)?;
            }
            let (rows, _) = term.size();
            let viewport = (rows as usize).saturating_sub(FOOTER_LINES + 1);
            self.adjust_scroll(viewport);
            let lines = self.render(viewport);
            lines.iter().for_each(|l| output.print(PIPE, l));
            lines_drawn = lines.len();
            let key = term.read_key().map_err(term_err)?;
            match self.handle_key(key, viewport) {
                Action::Continue => {},
                Action::Quit => {
                    term.clear_last_lines(lines_drawn).map_err(term_err)?;
                    return Ok(Outcome::Quit);
                },
                Action::Cleanup => {
                    term.clear_last_lines(lines_drawn).map_err(term_err)?;
                    return Ok(Outcome::Cleanup(self.collect_deletions()));
                },
                Action::OpenDiff(cluster, member) => {
                    term.clear_last_lines(lines_drawn).map_err(term_err)?;
                    lines_drawn = 0;
                    self.show_diff(term, output, cluster, member).await?;
                },
            }
        }
    }

    fn visible(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (ci, cluster) in self.clusters.iter().enumerate() {
            rows.push(Row::Header(ci));
            if cluster.expanded {
                for mi in 0..cluster.candidates.len() {
                    rows.push(Row::Member(ci, mi));
                }
            }
        }
        rows
    }

    fn total_visible_rows(&self) -> usize {
        self.clusters.iter().map(|c| 1 + if c.expanded { c.candidates.len() } else { 0 }).sum()
    }

    fn render(&'a self, viewport: usize) -> Vec<Line<'a>> {
        let visible = self.visible();
        let start = self.scroll_offset.min(visible.len());
        let end = (start + viewport).min(visible.len());
        let mut lines: Vec<Line<'a>> = Vec::new();
        for (i, row) in visible[start..end].iter().enumerate() {
            let is_cursor = start + i == self.cursor;
            lines.push(match *row {
                Row::Header(ci) => self.render_header(ci, is_cursor),
                Row::Member(ci, mi) => self.render_member(ci, mi, is_cursor),
            });
        }
        lines.push(Line::empty().with_volume(Loudness::Shout));
        lines.push(self.status());
        lines.push(self.help());
        lines
    }

    fn render_header(&'a self, ci: usize, cursor: bool) -> Line<'a> {
        let cluster = &self.clusters[ci];
        let keep = &cluster.candidates[0].version;
        let mut pieces: Vec<Piece<'a>> = vec![
            if cursor { Piece::fixed("\u{2192} ", &PALETTE.highlight) } else { Piece::plain("  ") },
            if cluster.expanded {
                Piece::fixed("\u{25be} ", &PALETTE.muted)
            } else {
                Piece::fixed("\u{25b8} ", &PALETTE.muted)
            },
        ];
        pieces.extend(crate::output::util::format_pair_pieces(None, None::<&File>, keep, self.fandoms));
        pieces.push(Piece::fixed(format!(" \u{00b7} {} near-dups", cluster.candidates.len()), &PALETTE.muted));
        let marked = cluster.marked();
        if marked > 0 {
            pieces.push(Piece::fixed(format!(" \u{00b7} {marked} to trash"), &PALETTE.danger));
        }
        Line::new(pieces).with_volume(Loudness::Shout)
    }

    fn render_member(&'a self, ci: usize, mi: usize, cursor: bool) -> Line<'a> {
        let cluster = &self.clusters[ci];
        let candidate = &cluster.candidates[mi];
        let version = &candidate.version;
        let is_keep = mi == 0;
        let mark = match candidate.mark {
            Mark::Delete => Piece::fixed("\u{2717} trash ", &PALETTE.removed),
            Mark::Keep if is_keep => Piece::fixed("\u{2605} keep  ", &PALETTE.success),
            Mark::Keep => Piece::fixed("  keep  ", &PALETTE.muted),
        };
        Line::new([
            if cursor { Piece::fixed("   \u{2192} ", &PALETTE.highlight) } else { Piece::plain("     ") },
            mark,
            Piece::fixed(format!("{:08x}", version.crc32), &PALETTE.accent),
            Piece::space(),
            Piece::fixed(format!("{:.1}k", version.metadata.words.max(101) as f32 / 1000.0), &PALETTE.highlight),
            Piece::space(),
            Piece::fixed(
                version.metadata.chapters.to_string(),
                if version.metadata.chapters.is_complete() { &PALETTE.success } else { &PALETTE.muted },
            ),
            Piece::space(),
            Piece::fixed(version.metadata.last_modified.to_string(), &PALETTE.muted),
            Piece::space(),
            Piece::fixed(format_bytes(version.length), &PALETTE.muted),
            Piece::fixed("  ", &PALETTE.muted),
            self.member_hint(cluster, candidate, is_keep),
        ])
        .with_volume(Loudness::Shout)
    }

    fn member_hint(&self, cluster: &Cluster, candidate: &Candidate, is_keep: bool) -> Piece<'static> {
        if is_keep {
            return Piece::fixed("recommended", &PALETTE.success);
        }
        if self.normalize {
            if candidate.identical {
                return Piece::fixed("\u{2261} identical", &PALETTE.success);
            }
            if let Some(ratio) = candidate.similarity {
                return Piece::fixed(format!("~{:.1}% similar", ratio * 100.0), &PALETTE.warning);
            }
        }
        let delta = candidate.version.length as i64 - cluster.keep_length() as i64;
        if delta == 0 {
            Piece::fixed("same size", &PALETTE.muted)
        } else {
            let sign = if delta > 0 { "+" } else { "-" };
            Piece::fixed(format!("{sign}{}", format_bytes(delta.unsigned_abs())), &PALETTE.muted)
        }
    }

    fn status(&self) -> Line<'a> {
        let marked: usize = self.clusters.iter().map(Cluster::marked).sum();
        Line::new([
            Piece::fixed(marked.to_string(), &PALETTE.highlight),
            Piece::fixed(" version(s) marked to trash", &PALETTE.muted),
        ])
        .with_volume(Loudness::Shout)
    }

    fn help(&self) -> Line<'a> {
        Line::new([Piece::fixed(
            "\u{2191}\u{2193} move \u{00b7} Space expand/mark \u{00b7} Enter diff \u{00b7} d trash marked \u{00b7} q quit",
            &PALETTE.muted,
        )])
        .with_volume(Loudness::Shout)
    }

    fn handle_key(&mut self, key: Key, viewport: usize) -> Action {
        let visible = self.visible();
        let max_idx = visible.len().saturating_sub(1);
        let current = visible.get(self.cursor).copied();
        match key {
            Key::ArrowUp | Key::Char('k') => self.cursor = self.cursor.saturating_sub(1),
            Key::ArrowDown | Key::Char('j') => self.cursor = self.cursor.saturating_add(1).min(max_idx),
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = max_idx,
            Key::PageUp => self.cursor = self.cursor.saturating_sub(viewport),
            Key::PageDown => self.cursor = (self.cursor + viewport).min(max_idx),
            Key::Char(' ') => match current {
                Some(Row::Header(ci)) => {
                    self.clusters[ci].expanded = !self.clusters[ci].expanded;
                    self.clamp_cursor();
                },
                Some(Row::Member(ci, mi)) => self.toggle_member(ci, mi),
                None => {},
            },
            Key::Enter => match current {
                Some(Row::Header(ci)) => {
                    self.clusters[ci].expanded = !self.clusters[ci].expanded;
                    self.clamp_cursor();
                },
                Some(Row::Member(ci, mi)) if mi > 0 => return Action::OpenDiff(ci, mi),
                _ => {},
            },
            Key::Char('d') => return Action::Cleanup,
            Key::Escape | Key::Char('q') | Key::CtrlC => return Action::Quit,
            _ => {},
        }
        Action::Continue
    }

    /// Toggle a member's keep/delete mark, refusing to delete the last surviving
    /// version of a cluster.
    fn toggle_member(&mut self, ci: usize, mi: usize) {
        let cluster = &mut self.clusters[ci];
        match cluster.candidates[mi].mark {
            Mark::Keep => {
                if cluster.candidates.iter().filter(|c| c.mark == Mark::Keep).count() <= 1 {
                    return;
                }
                cluster.candidates[mi].mark = Mark::Delete;
            },
            Mark::Delete => cluster.candidates[mi].mark = Mark::Keep,
        }
    }

    fn clamp_cursor(&mut self) {
        let max = self.total_visible_rows().saturating_sub(1);
        if self.cursor > max {
            self.cursor = max;
        }
    }

    fn adjust_scroll(&mut self, viewport: usize) {
        if viewport == 0 {
            return;
        }
        let total = self.total_visible_rows();
        let max_offset = total.saturating_sub(viewport);
        self.scroll_offset = self.scroll_offset.min(max_offset);
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        }
        if self.cursor >= self.scroll_offset + viewport {
            self.scroll_offset = self.cursor - viewport + 1;
        }
    }

    fn collect_deletions(&self) -> Vec<Deletion> {
        self.clusters
            .iter()
            .flat_map(|cluster| {
                cluster.candidates.iter().filter(|c| c.mark == Mark::Delete).map(move |candidate| Deletion {
                    content_hash: candidate.version.hash.clone(),
                    files: candidate.files.clone(),
                    label: format!("#{} {:08x}", cluster.work_id, candidate.version.crc32),
                })
            })
            .collect()
    }

    async fn show_diff(&self, term: &Term, output: &dyn Output, ci: usize, mi: usize) -> Result<()> {
        let keep = &self.clusters[ci].candidates[0];
        let other = &self.clusters[ci].candidates[mi];
        let keep_text = match read_member_text(keep, &self.backends, self.normalize).await {
            Ok(text) => text,
            Err(e) => return self.flash(term, output, &format!("Failed to read keep version: {e}")),
        };
        let other_text = match read_member_text(other, &self.backends, self.normalize).await {
            Ok(text) => text,
            Err(e) => return self.flash(term, output, &format!("Failed to read version: {e}")),
        };
        let diff = compute_diff(&keep_text, &other_text);
        let header = self.diff_header(ci, mi, &diff);
        let help = Line::new([Piece::fixed(
            "\u{2191}\u{2193} scroll \u{00b7} q/Esc back",
            &PALETTE.muted,
        )])
        .with_volume(Loudness::Shout);

        let mut offset = 0usize;
        let mut drawn = 0usize;
        loop {
            if drawn > 0 {
                term.clear_last_lines(drawn).map_err(term_err)?;
            }
            let (rows, _) = term.size();
            let budget = (rows as usize).saturating_sub(header.len() + 2);
            let max_offset = diff.lines.len().saturating_sub(budget);
            offset = offset.min(max_offset);

            header.iter().for_each(|l| output.print(PIPE, l));
            let end = (offset + budget).min(diff.lines.len());
            let body = &diff.lines[offset..end];
            body.iter().for_each(|l| output.print(PIPE, l));
            if diff.lines.is_empty() {
                output.print(
                    PIPE,
                    &Line::new([Piece::fixed("No differences \u{2014} identical content.", &PALETTE.success)])
                        .with_volume(Loudness::Shout),
                );
            }
            output.print(PIPE, &help);
            drawn = header.len() + body.len() + if diff.lines.is_empty() { 1 } else { 0 } + 1;

            match term.read_key().map_err(term_err)? {
                Key::ArrowUp | Key::Char('k') => offset = offset.saturating_sub(1),
                Key::ArrowDown | Key::Char('j') => offset = (offset + 1).min(max_offset),
                Key::PageUp => offset = offset.saturating_sub(budget),
                Key::PageDown => offset = (offset + budget).min(max_offset),
                Key::Home => offset = 0,
                Key::End => offset = max_offset,
                Key::Enter | Key::Escape | Key::Char('q') | Key::CtrlC => {
                    term.clear_last_lines(drawn).map_err(term_err)?;
                    return Ok(());
                },
                _ => {},
            }
        }
    }

    fn diff_header(&self, ci: usize, mi: usize, diff: &super::content::DiffResult) -> Vec<Line<'static>> {
        let cluster = &self.clusters[ci];
        let keep = &cluster.candidates[0].version;
        let other = &cluster.candidates[mi].version;
        let verdict = if diff.identical {
            "identical".to_string()
        } else {
            format!("~{:.1}% similar \u{00b7} +{} \u{2212}{}", diff.ratio * 100.0, diff.additions, diff.deletions)
        };
        vec![
            Line::new([
                Piece::fixed(format!("#{}", cluster.work_id), &PALETTE.highlight),
                Piece::space(),
                Piece::fixed(cluster.title.clone(), &PALETTE.success),
            ])
            .with_volume(Loudness::Shout),
            Line::new([
                Piece::fixed(format!("keep {:08x}", keep.crc32), &PALETTE.success),
                Piece::fixed("  \u{27f7}  ", &PALETTE.muted),
                Piece::fixed(format!("{:08x}", other.crc32), &PALETTE.accent),
                Piece::fixed("   ", &PALETTE.muted),
                Piece::fixed(verdict, if diff.identical { &PALETTE.success } else { &PALETTE.warning }),
                Piece::fixed(if self.normalize { "  (normalised)" } else { "  (raw)" }, &PALETTE.muted),
            ])
            .with_volume(Loudness::Shout),
            Line::empty().with_volume(Loudness::Shout),
        ]
    }

    fn flash(&self, term: &Term, output: &dyn Output, message: &str) -> Result<()> {
        output.print(
            PIPE,
            &Line::new([Piece::fixed(message.to_string(), &PALETTE.danger)]).with_volume(Loudness::Shout),
        );
        let _ = term.read_key();
        term.clear_last_lines(1).map_err(term_err)?;
        Ok(())
    }
}

/// Reserved bottom rows: blank, status, help.
const FOOTER_LINES: usize = 3;
