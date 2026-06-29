use console::Style;
use std::sync::LazyLock;

/// Shared default `Palette`, lazily initialised on first use and reused across
/// the CLI.
///
/// Each field is a [`Style`] that [`Piece::fixed`](crate::Piece::fixed) and
/// [`Piece::flex`](crate::Piece::flex) accept by reference.
///
/// # Examples
///
/// ```
/// use rawr_output::{Piece, PALETTE};
/// let piece = Piece::fixed("Title", &PALETTE.heading);
/// ```
pub static PALETTE: LazyLock<Palette> = LazyLock::new(Palette::default);

/// Semantic styling roles applied to a [`Piece`](crate::Piece).
///
/// Roles name an intent rather than a colour, so the look can change in one place.
/// The shared instance is [`PALETTE`].
pub struct Palette {
    /// Section or document titles.
    pub heading: Style,
    /// Confirmation that an operation completed.
    pub success: Style,
    /// Recoverable problems the user should notice.
    pub warning: Style,
    /// Errors and failures.
    pub danger: Style,
    /// Secondary, de-emphasised text.
    pub muted: Style,
    /// Emphasis to draw the eye.
    pub highlight: Style,
    /// Field names in label and value pairs.
    pub label: Style,
    /// Distinguishing accent for selected values.
    pub accent: Style,
    /// Additions in a diff.
    pub added: Style,
    /// Removals in a diff.
    pub removed: Style,
}
/// Builds the role styles backing [`PALETTE`].
impl Default for Palette {
    fn default() -> Self {
        Self {
            heading: Style::new().green().bold().underlined(),
            success: Style::new().green(),
            warning: Style::new().yellow(),
            danger: Style::new().red(),
            muted: Style::new().dim(),
            highlight: Style::new().bright().bold(),
            label: Style::new().cyan(),
            accent: Style::new().magenta(),
            added: Style::new().green(),
            removed: Style::new().red(),
        }
    }
}
