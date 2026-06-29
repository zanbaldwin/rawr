//! The rendering contract: turning [`Line`]s and [`Piece`](crate::Piece)s into
//! displayable strings.

pub(super) mod line;
pub(super) mod piece;

use self::line::Line;
use std::borrow::Cow;

/// Turns a value into its displayable string for a given width and colour setting.
///
/// `width` of `None` means the natural width with no truncation; `Some(n)` fits
/// the output to `n` columns. `colour` of `false` drops all styling and renders
/// the underlying text verbatim. The result is a [`Cow`] so unstyled, unconstrained
/// text can be borrowed without allocating.
///
/// Implemented by [`Line`] and [`Piece`](crate::Piece).
///
/// # Examples
/// ```
/// use rawr_output::{Line, Piece, Render, PALETTE};
/// let line = Line::new([
///     Piece::fixed("hello", &PALETTE.heading),
///     Piece::space(),
///     Piece::plain("world"),
/// ]);
/// // Without colour, styling is dropped and pieces render verbatim.
/// assert_eq!(&*line.render(None, false), "hello world");
/// ```
pub trait Render<'a> {
    /// Produces the displayable string for the target `width` (`None` = natural
    /// width) with styling included only when `colour` is `true`.
    fn render(&'a self, width: Option<usize>, colour: bool) -> Cow<'a, str>;
}

/// Yields the [`Line`] representation of a domain type so it can be printed.
///
/// Callers implement this on their own types (for example `impl IntoLines for
/// AppContext`), then feed each line to an [`Output`](crate::Output).
pub trait IntoLines {
    /// Builds the lines that represent this value.
    fn to_lines(&self) -> Vec<Line<'_>>;
}
