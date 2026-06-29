use crate::error::Result;
use crate::output::util::format_pair_pieces;
use console::Key;
use rawr_cache::Repository;
use rawr_config::models::FandomConfig;
use rawr_extract::models::Version;
use rawr_output::{Line, Loudness, Output, PALETTE, Piece, Pipe};
use rawr_storage::file::{FileInfo, Processed};
use std::sync::Arc;

type File = FileInfo<Processed>;
type VersionFiles = (Version, Vec<File>);
type WorkVersions = (u64, Vec<VersionFiles>);

const PICKER_PIPE: Pipe = Pipe::Err;

pub struct Picker<'a> {
    fandoms: Option<&'a FandomConfig>,
    works: Vec<WorkEntry>,
    cursor: usize,
    scroll_offset: usize,
}
impl<'a> Picker<'a> {
    pub async fn interact(
        target: &str,
        cache: Arc<Repository>,
        output: Arc<dyn Output>,
        fandoms: impl Into<Option<&'a FandomConfig>>,
        limit: usize,
    ) -> Result<Vec<VersionFiles>> {
        if !output.is_interactive(PICKER_PIPE) {
            let line = Line::new([Piece::fixed(
                "Interactive mode requires a terminal. Provide work references as arguments instead.",
                &PALETTE.warning,
            )]);
            output.print_to(PICKER_PIPE, &line.with_volume(Loudness::Shout));
            return Ok(Vec::new());
        }
        let works = WorkEntry::build(cache.list_recent_works_for_target(target, limit).await?);
        if works.is_empty() {
            let line = Line::new([Piece::fixed(
                "No works found in the library. Run `rawr scan` or `rawr import` first.",
                &PALETTE.warning,
            )]);
            output.print_to(PICKER_PIPE, &line.with_volume(Loudness::Shout));
            return Ok(Vec::new());
        }
        let (_guard, term) = output.alt(PICKER_PIPE)?;
        let term_err = |e: std::io::Error| miette::miette!("Terminal error: {e}");
        let mut state = Self {
            fandoms: fandoms.into(),
            works,
            cursor: 0,
            scroll_offset: 0,
        };
        let mut lines_drawn: usize = 0;
        loop {
            if lines_drawn > 0 {
                term.clear_last_lines(lines_drawn).map_err(term_err)?;
            }
            let (term_rows, _) = term.size();
            let viewport = (term_rows as usize).saturating_sub(1);
            state.adjust_scroll(viewport);
            let lines = state.render(viewport);
            lines.iter().for_each(|l| output.print_to(PICKER_PIPE, l));
            lines_drawn = lines.len();
            let key = term.read_key().map_err(term_err)?;
            match state.handle_key(key, viewport) {
                Action::Continue => {},
                Action::Confirm => {
                    term.clear_last_lines(lines_drawn).map_err(term_err)?;
                    return Ok(state.collect());
                },
                Action::Cancel => {
                    term.clear_last_lines(lines_drawn).map_err(term_err)?;
                    return Ok(Vec::new());
                },
            }
        }
    }

    fn visible_rows(&self) -> Vec<RowRef<'_>> {
        let mut rows = Vec::new();
        for (i, work) in self.works.iter().enumerate() {
            let Some((best, _files)) = work.versions.first() else {
                continue;
            };
            rows.push(RowRef::Work(i, best, work.versions.len(), work.selected));
            if work.expanded {
                for (j, version_files) in work.versions.iter().enumerate() {
                    let (version, files) = version_files;
                    let Some(file) = files.first() else {
                        continue;
                    };
                    rows.push(RowRef::Version(i, j, version, file, j == work.selected_version));
                }
            }
        }
        rows
    }

    fn total_visible_rows(&self) -> usize {
        self.works.iter().map(|w| 1 + if w.expanded { w.versions.len() } else { 0 }).sum()
    }

    fn render(&'a self, viewport: usize) -> Vec<Line<'a>> {
        let visible = self.visible_rows();
        let end = (self.scroll_offset + viewport).min(visible.len());
        let mut lines = vec![];
        for (i, row) in visible[self.scroll_offset..end].iter().enumerate() {
            let is_cursor = self.scroll_offset + i == self.cursor;
            let line = row.render(self.fandoms, is_cursor);
            lines.push(line);
        }
        lines.push(Line::empty());
        lines.push(self.status());
        lines
    }

    fn status(&self) -> Line<'_> {
        Line::new([
            Piece::fixed(self.works.iter().filter(|w| w.selected).count().to_string(), &PALETTE.highlight),
            Piece::fixed(" selected · ", &PALETTE.muted),
            Piece::plain("Space"),
            Piece::fixed(": ", &PALETTE.muted),
            Piece::fixed("toggle", &PALETTE.highlight),
            Piece::fixed(" · ", &PALETTE.muted),
            Piece::plain("Enter"),
            Piece::fixed(": ", &PALETTE.muted),
            Piece::fixed("confirm", &PALETTE.highlight),
            Piece::fixed(" · ", &PALETTE.muted),
            Piece::plain("q"),
            Piece::fixed("/", &PALETTE.muted),
            Piece::plain("Esc"),
            Piece::fixed(": ", &PALETTE.muted),
            Piece::fixed("cancel", &PALETTE.highlight),
        ])
    }

    fn handle_key(&mut self, key: Key, viewport: usize) -> Action {
        let mut visible = self.visible_rows();
        let max_idx = visible.len().saturating_sub(1);
        let target_row: Option<CursorTarget> =
            if self.cursor <= max_idx { Some(visible.swap_remove(self.cursor).into()) } else { None };
        match key {
            Key::ArrowUp | Key::Char('k') => self.cursor = self.cursor.saturating_sub(1),
            Key::ArrowDown | Key::Char('j') => self.cursor = self.cursor.saturating_add(1).min(max_idx),
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = max_idx,
            Key::PageUp => self.cursor = self.cursor.saturating_sub(viewport),
            Key::PageDown => self.cursor = (self.cursor + viewport).min(max_idx),
            Key::Char(' ') => match target_row {
                Some(CursorTarget::Work(idx, was_selected)) => {
                    let work = &mut self.works[idx];
                    work.selected = !was_selected;
                    work.expanded = work.selected && work.versions.len() > 1;
                    let new_max = self.total_visible_rows().saturating_sub(1);
                    if self.cursor > new_max {
                        self.cursor = new_max;
                    }
                },
                Some(CursorTarget::Version(i, j)) => {
                    let work = &mut self.works[i];
                    work.selected_version = j;
                    work.selected = true;
                },
                None => {},
            },
            Key::Enter => return Action::Confirm,
            Key::Escape | Key::Char('q') | Key::CtrlC => return Action::Cancel,
            _ => {},
        };
        Action::Continue
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

    fn collect(self) -> Vec<VersionFiles> {
        self.works
            .into_iter()
            .filter(|w| w.selected && w.selected_version < w.versions.len())
            .map(|mut w| w.versions.swap_remove(w.selected_version))
            .collect()
    }
}

struct WorkEntry {
    work_id: u64,
    versions: Vec<VersionFiles>,
    selected: bool,
    expanded: bool,
    selected_version: usize,
}
impl WorkEntry {
    fn build(works: Vec<WorkVersions>) -> Vec<WorkEntry> {
        works.into_iter().filter_map(|wv| wv.try_into().ok()).collect()
    }
}
impl TryFrom<WorkVersions> for WorkEntry {
    type Error = ();
    fn try_from(value: WorkVersions) -> std::result::Result<Self, Self::Error> {
        let (work_id, versions) = value;
        let versions: Vec<_> = versions.into_iter().filter(|(_version, files)| !files.is_empty()).collect();
        if versions.is_empty() {
            return Err(());
        }
        Ok(WorkEntry {
            work_id,
            versions,
            selected: false,
            expanded: false,
            selected_version: 0,
        })
    }
}

enum RowRef<'a> {
    Work(usize, &'a Version, usize, bool),
    Version(usize, usize, &'a Version, &'a File, bool),
}
impl<'a> RowRef<'a> {
    fn render(&self, fandoms: impl Into<Option<&'a FandomConfig>>, cursor: bool) -> Line<'a> {
        match self {
            Self::Work(_idx, version, count, selected) => {
                Self::render_work(version, fandoms, *selected, *count, cursor)
            },
            Self::Version(_i, _j, version, file, selected) => Self::render_version(version, file, *selected, cursor),
        }
        .with_volume(Loudness::Shout)
    }

    fn render_work(
        work: &'a Version,
        fandoms: impl Into<Option<&'a FandomConfig>>,
        selected: bool,
        count: usize,
        cursor: bool,
    ) -> Line<'a> {
        let mut pieces: Vec<Piece<'_>> = vec![
            if cursor { Piece::fixed("→ ", &PALETTE.highlight) } else { Piece::plain("  ") },
            if selected { Piece::fixed("[✓] ", &PALETTE.success) } else { Piece::plain("[ ] ") },
        ];
        pieces.extend(format_pair_pieces(None, None::<&File>, work, fandoms));
        if count > 1 {
            pieces.push(Piece::fixed(" +", &PALETTE.muted));
        }
        Line::new(pieces)
    }

    fn render_version(version: &'a Version, file: &'a File, selected: bool, cursor: bool) -> Line<'a> {
        Line::new([
            if cursor { Piece::fixed("→  ⇢  ", &PALETTE.highlight) } else { Piece::plain("      ") },
            if selected { Piece::fixed("(•) ", &PALETTE.accent) } else { Piece::fixed("( ) ", &PALETTE.muted) },
            Piece::fixed(format!("{:08x}", version.crc32), &PALETTE.accent),
            Piece::space(),
            Piece::fixed(format!("{:.1}k", version.metadata.words.max(101) as f32 / 1000.0), &PALETTE.highlight),
            Piece::space(),
            Piece::fixed(
                version.metadata.chapters.to_string(),
                if version.metadata.chapters.is_complete() { &PALETTE.success } else { &PALETTE.danger },
            ),
            Piece::space(),
            Piece::fixed(version.metadata.last_modified.to_string(), &PALETTE.muted),
            Piece::space(),
            Piece::flex(file.path.to_string(), &PALETTE.muted, 16),
        ])
    }
}

#[derive(Clone, Copy)]
enum CursorTarget {
    Work(usize, bool),
    Version(usize, usize),
}
impl From<RowRef<'_>> for CursorTarget {
    fn from(value: RowRef) -> Self {
        match value {
            RowRef::Work(idx, _, _, selected) => Self::Work(idx, selected),
            RowRef::Version(i, j, _, _, _) => Self::Version(i, j),
        }
    }
}

enum Action {
    Continue,
    Confirm,
    Cancel,
}
