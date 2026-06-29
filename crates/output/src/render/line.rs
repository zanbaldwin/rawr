use super::Render;
use super::piece::{Flexibility, Piece};
use crate::Verbosity;
use std::borrow::Cow;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Loudness {
    Whisper,
    #[default]
    Normal,
    Shout,
}
impl Loudness {
    pub fn is_visible(&self, verbosity: Verbosity) -> bool {
        match self {
            Self::Whisper => matches!(verbosity, Verbosity::Verbose),
            Self::Normal => !matches!(verbosity, Verbosity::Quiet),
            Self::Shout => true,
        }
    }
}

#[derive(Default)]
pub struct Line<'a> {
    pub loudness: Loudness,
    pieces: Vec<Piece<'a>>,
    fallback: Option<Cow<'a, str>>,
}
impl<'a> Line<'a> {
    pub fn new(pieces: impl IntoIterator<Item = Piece<'a>>) -> Self {
        Self {
            loudness: Loudness::default(),
            pieces: pieces.into_iter().collect(),
            fallback: None,
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn with_volume(mut self, loudness: Loudness) -> Self {
        self.loudness = loudness;
        self
    }

    pub fn with_fallback<S: Into<Cow<'a, str>>>(mut self, fallback: impl Into<Option<S>>) -> Self {
        self.fallback = fallback.into().map(|s| s.into());
        self
    }

    pub fn is_visible(&self, verbosity: Verbosity) -> bool {
        self.loudness.is_visible(verbosity)
    }

    pub fn push(&mut self, piece: Piece<'a>) {
        self.pieces.push(piece)
    }
}

impl From<Loudness> for Line<'_> {
    fn from(value: Loudness) -> Self {
        Self::empty().with_volume(value)
    }
}
impl<'a, I> From<I> for Line<'a>
where
    I: IntoIterator<Item = Piece<'a>>,
{
    fn from(value: I) -> Self {
        Line::new(value)
    }
}

#[derive(Clone)]
struct FlexEntry {
    index: usize,
    natural_width: usize,
    min_width: usize,
}
impl FlexEntry {
    fn elasticity(&self) -> usize {
        self.natural_width.saturating_sub(self.min_width)
    }
}

/// Proportionally allocate `budget` among `entries` based on elasticity.
///
/// Each entry is guaranteed its `min_width` first; surplus budget is distributed
/// proportionally by elasticity (`natural_width - min_width`). When total minimums
/// exceed the budget, the least-elastic entry is dropped (assigned zero) and the
/// budget is redistributed among the remaining entries.
/// Returns `(piece_index, allocated_width)` pairs for every entry.
fn allocate_flex_budgets(entries: &[FlexEntry], budget: usize) -> Vec<(usize, usize)> {
    let mut active = entries.to_vec();
    let mut result = Vec::new();

    loop {
        let total_min: usize = active.iter().map(|e| e.min_width).sum();
        if total_min > budget {
            let drop_idx = if let Some(entry) = active.iter().min_by_key(|e| e.elasticity()) {
                entry.index
            } else {
                return result;
            };
            result.push((drop_idx, 0));
            active.retain(|e| e.index != drop_idx);
            continue;
        }
        let surplus_budget = budget - total_min;
        let total_elasticity: usize = active.iter().map(|e| e.elasticity()).sum();
        if total_elasticity == 0 {
            result.extend(active.iter().map(|e| (e.index, e.min_width)));
            return result;
        }
        let mut allocs: Vec<(usize, usize)> = active
            .iter()
            .map(|e| {
                let elasticity = e.elasticity();
                (e.index, e.min_width + surplus_budget * elasticity / total_elasticity)
            })
            .collect();
        let allocated: usize = allocs.iter().map(|(_, a)| a).sum();
        for (_, alloc) in allocs.iter_mut().take(budget - allocated) {
            *alloc += 1;
        }
        result.extend(allocs);
        return result;
    }
}

impl<'a> Render<'a> for Line<'a> {
    fn render(&'a self, width: Option<usize>, colour: bool) -> Cow<'a, str> {
        if !colour && let Some(ref fallback) = self.fallback {
            return match fallback {
                Cow::Owned(s) => Cow::Owned(s.clone()),
                Cow::Borrowed(s) => Cow::Borrowed(*s),
            };
        }
        if self.pieces.is_empty() {
            return Cow::Borrowed("");
        }
        if self.pieces.len() == 1 {
            return self.pieces[0].render(width, colour);
        }
        // Compute per-piece width budgets (None = render at natural width)
        let mut budgets: Vec<Option<usize>> = vec![None; self.pieces.len()];
        if let Some(w) = width {
            // Classify pieces into fixed-width and flexible
            let mut fixed_width: usize = 0;
            let mut flex_entries: Vec<FlexEntry> = Vec::new();
            for (index, piece) in self.pieces.iter().enumerate() {
                let natural_width = piece.width();
                match piece.flex {
                    Flexibility::Fixed => fixed_width += natural_width,
                    Flexibility::Truncatable(min) if natural_width <= min => fixed_width += natural_width,
                    Flexibility::Truncatable(min_width) => {
                        flex_entries.push(FlexEntry { index, natural_width, min_width });
                    },
                }
            }
            let remaining = w.saturating_sub(fixed_width);
            let total_nat: usize = flex_entries.iter().map(|e| e.natural_width).sum();
            if total_nat > remaining {
                for (idx, alloc) in allocate_flex_budgets(&flex_entries, remaining) {
                    budgets[idx] = Some(alloc);
                }
            }
        }
        // Render each piece with its computed budget
        Cow::Owned(self.pieces.iter().zip(budgets).map(|(piece, budget)| piece.render(budget, colour)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::Style;

    #[test]
    fn render_empty_line_returns_empty_string() {
        let line = Line::empty();
        assert_eq!(line.render(None, false), "");
    }

    #[test]
    fn render_empty_line_with_width_returns_empty_string() {
        let line = Line::empty();
        assert_eq!(line.render(Some(80), false), "");
    }

    #[test]
    fn render_single_fixed_piece_no_width() {
        let line = Line::new([Piece::plain("hello")]);
        assert_eq!(line.render(None, false), "hello");
    }

    #[test]
    fn render_single_fixed_piece_with_width_does_not_truncate() {
        let line = Line::new([Piece::plain("hello world")]);
        // Fixed pieces are never truncated, even when exceeding width
        assert_eq!(line.render(Some(5), false), "hello world");
    }

    #[test]
    fn render_single_fixed_piece_with_colour() {
        let style = Style::new().bold();
        let line = Line::new([Piece::fixed("hello", &style)]);
        let rendered = line.render(None, true);
        assert_ne!(rendered.as_ref(), "hello");
        assert!(rendered.contains("hello"));
    }

    #[test]
    fn render_single_truncatable_piece_no_width() {
        let line = Line::new([Piece::flex("hello world", Style::new(), 3)]);
        assert_eq!(line.render(None, false), "hello world");
    }

    #[test]
    fn render_single_truncatable_piece_fits_within_width() {
        let line = Line::new([Piece::flex("hello", Style::new(), 3)]);
        assert_eq!(line.render(Some(20), false), "hello");
    }

    #[test]
    fn render_single_truncatable_piece_exceeds_width() {
        let line = Line::new([Piece::flex("hello world", Style::new(), 3)]);
        let rendered = line.render(Some(5), false);
        assert_eq!(console::measure_text_width(&rendered), 5);
    }

    #[test]
    fn render_single_truncatable_piece_min_width_exceeds_available() {
        let line = Line::new([Piece::flex("hello world", Style::new(), 10)]);
        let rendered = line.render(Some(5), false);
        assert_eq!(rendered, "");
    }

    #[test]
    fn render_multiple_fixed_pieces_no_width() {
        let line = Line::new([Piece::plain("hello"), Piece::space(), Piece::plain("world")]);
        assert_eq!(line.render(None, false), "hello world");
    }

    #[test]
    fn render_multiple_fixed_pieces_with_width_does_not_truncate() {
        let line = Line::new([Piece::plain("hello"), Piece::space(), Piece::plain("world")]);
        // No truncatable pieces, so width constraint has no effect
        assert_eq!(line.render(Some(5), false), "hello world");
    }

    #[test]
    fn render_truncatable_among_fixed_fits() {
        // "[" + truncatable("hello", min=3) + "]" with width=20
        let line = Line::new([
            Piece::plain("["),
            Piece::flex("hello", Style::new(), 3),
            Piece::plain("]"),
        ]);
        // Fixed pieces consume 2 chars, leaving 18 for truncatable; "hello" is 5, fits
        assert_eq!(line.render(Some(20), false), "[hello]");
    }

    #[test]
    fn render_truncatable_among_fixed_needs_truncation() {
        // "[" + truncatable("hello world", min=3) + "]" with width=8
        let line = Line::new([
            Piece::plain("["),
            Piece::flex("hello world", Style::new(), 3),
            Piece::plain("]"),
        ]);
        // Fixed pieces consume 2 chars, leaving 6 for truncatable
        let rendered = line.render(Some(8), false);
        // Truncatable should be truncated to 6 chars
        assert_eq!(console::measure_text_width(&rendered), 8);
        assert!(rendered.starts_with('['));
        assert!(rendered.ends_with(']'));
    }

    #[test]
    fn render_truncatable_among_fixed_dropped_due_to_min_width() {
        // "prefix:" + truncatable("hello world", min=10) + " suffix" with width=12
        let line = Line::new([
            Piece::plain("prefix:"),
            Piece::flex("hello world", Style::new(), 10),
            Piece::plain(" suffix"),
        ]);
        // Fixed pieces consume 7 + 7 = 14 chars, leaving 0 for truncatable (12 - 14 saturates to 0)
        // min_width=10 > 0, so truncatable is dropped (rendered as "")
        let rendered = line.render(Some(12), false);
        assert_eq!(rendered, "prefix: suffix");
    }

    #[test]
    fn render_two_truncatable_pieces_both_fit() {
        // Two short truncatable pieces that both fit within the width
        let line = Line::new([
            Piece::flex("ab", Style::new(), 1),
            Piece::plain("|"),
            Piece::flex("cd", Style::new(), 1),
        ]);
        // Total content: 2 + 1 + 2 = 5; width=20 => everything fits
        assert_eq!(line.render(Some(20), false), "ab|cd");
    }

    #[test]
    fn render_two_truncatable_pieces_total_exceeds_width() {
        // Two truncatable pieces whose combined length exceeds width.
        // The total rendered width must not exceed the constraint.
        let line = Line::new([
            Piece::flex("hello world", Style::new(), 3),
            Piece::plain("|"),
            Piece::flex("foo bar baz", Style::new(), 3),
        ]);
        // Total unconstrained: 11 + 1 + 11 = 23; width=15
        // Fixed: 1 char ("|"), remaining: 14 for two truncatable pieces
        let rendered = line.render(Some(15), false);
        assert!(
            console::measure_text_width(&rendered) <= 15,
            "rendered width {} exceeds constraint 15: {:?}",
            console::measure_text_width(&rendered),
            rendered,
        );
    }

    #[test]
    fn render_two_truncatable_pieces_first_is_not_ignored() {
        // Verify that the first truncatable piece is also subject to
        // width constraints, not just the last one.
        let line = Line::new([
            Piece::flex("aaaaaaaaaa", Style::new(), 2), // 10 chars, min 2
            Piece::plain("|"),                          // 1 char fixed
            Piece::flex("bb", Style::new(), 2),         // 2 chars, min 2
        ]);
        // width=7: fixed=1, remaining=6 for two truncatables
        // If only the last truncatable gets a budget, the first renders
        // at full 10 chars and total becomes 10+1+2 = 13, exceeding 7.
        // Correct: both truncatables must share the 6 remaining chars.
        let rendered = line.render(Some(7), false);
        assert!(
            console::measure_text_width(&rendered) <= 7,
            "rendered width {} exceeds constraint 7: {:?}",
            console::measure_text_width(&rendered),
            rendered,
        );
    }

    #[test]
    fn render_three_truncatable_pieces_respects_total_width() {
        let line = Line::new([
            Piece::flex("alpha", Style::new(), 2),
            Piece::plain("-"),
            Piece::flex("bravo", Style::new(), 2),
            Piece::plain("-"),
            Piece::flex("charlie", Style::new(), 2),
        ]);
        // Unconstrained: 5+1+5+1+7 = 19; width=11
        // Fixed: 2 ("-" x2), remaining: 9 for three truncatables
        let rendered = line.render(Some(11), false);
        assert!(
            console::measure_text_width(&rendered) <= 11,
            "rendered width {} exceeds constraint 11: {:?}",
            console::measure_text_width(&rendered),
            rendered,
        );
    }

    #[test]
    fn render_two_truncatable_pieces_reallocates_width_budget() {
        let line: Line = [
            Piece::plain("· "),
            Piece::plain("#12345678"),
            Piece::space(),
            Piece::flex("Short Title", Style::new(), 15),
            Piece::space(),
            Piece::plain("Some Fandom"),
            Piece::space(),
            Piece::plain("4.7k"),
            Piece::space(),
            Piece::plain("11/?"),
            Piece::space(),
            Piece::flex("some-fandom/12345-series-name/12345678-abcdef12-short-title.html.bz2", Style::new(), 16),
        ]
        .into();
        assert_eq!(
            line.render(Some(114), false),
            "· #12345678 Short Title Some Fandom 4.7k 11/? some-fandom/12345-series-name/12345678-abcdef12-short-title.html.bz2"
        );
        assert_eq!(
            line.render(Some(113), false),
            "· #12345678 Short Title Some Fandom 4.7k 11/? some-fandom/12345-series-name/12345678-abcdef12-short-title.html.b…"
        );
    }

    #[test]
    fn test_two_truncatable_pieces_short_flex_treated_as_fixed() {
        let line: Line = [
            Piece::plain("· "),
            Piece::plain("#12345678"),
            Piece::space(),
            Piece::flex("Shorter Title", Style::new(), 15),
            Piece::space(),
            Piece::plain("Some Fandom"),
            Piece::space(),
            Piece::plain("4.7k"),
            Piece::space(),
            Piece::plain("11/?"),
            Piece::space(),
            Piece::flex("some-fandom/12345-series-name/12345678-abcdef12-shorter-title.html.bz2", Style::new(), 16),
        ]
        .into();
        assert_eq!(
            line.render(Some(118), false),
            "· #12345678 Shorter Title Some Fandom 4.7k 11/? some-fandom/12345-series-name/12345678-abcdef12-shorter-title.html.bz2"
        );
        assert_eq!(
            line.render(Some(105), false),
            "· #12345678 Shorter Title Some Fandom 4.7k 11/? some-fandom/12345-series-name/12345678-abcdef12-shorter-…"
        );
    }

    #[test]
    fn from_iterator_creates_normal_loudness_line() {
        let line: Line = vec![Piece::plain("a"), Piece::plain("b")].into();
        assert_eq!(line.loudness, Loudness::Normal);
        assert_eq!(line.render(None, false), "ab");
    }

    #[test]
    fn push_appends_piece_to_line() {
        let mut line = Line::empty();
        line.push(Piece::plain("hello"));
        line.push(Piece::space());
        line.push(Piece::plain("world"));
        assert_eq!(line.render(None, false), "hello world");
    }

    #[test]
    fn loud_visible_at_all_verbosity_levels() {
        assert!(Loudness::Shout.is_visible(Verbosity::Quiet));
        assert!(Loudness::Shout.is_visible(Verbosity::Normal));
        assert!(Loudness::Shout.is_visible(Verbosity::Verbose));
    }

    #[test]
    fn normal_hidden_when_quiet() {
        assert!(!Loudness::Normal.is_visible(Verbosity::Quiet));
        assert!(Loudness::Normal.is_visible(Verbosity::Normal));
        assert!(Loudness::Normal.is_visible(Verbosity::Verbose));
    }

    #[test]
    fn quiet_only_visible_when_verbose() {
        assert!(!Loudness::Whisper.is_visible(Verbosity::Quiet));
        assert!(!Loudness::Whisper.is_visible(Verbosity::Normal));
        assert!(Loudness::Whisper.is_visible(Verbosity::Verbose));
    }
}
