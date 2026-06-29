//! Width-aware, styled, verbosity-filtered terminal output for the rawr CLI.
//!
//! Two layers sit on top of each other. The render layer describes WHAT a row of
//! output looks like: a [`Line`] is one row built from styled [`Piece`]s, [`Render`]
//! turns a line or piece into a string for an optional width and a colour flag,
//! and [`IntoLines`] lets a domain type produce its own `Vec<Line>`. The sink
//! layer decides WHERE lines go: [`Output`] is the destination trait, [`PrintingOutput`]
//! is the real stdout/stderr backend, and [`Pipe`] picks the stream.
//!
//! Every line carries a [`Loudness`]; an [`Output`] drops it when it is not visible
//! at the active [`Verbosity`]. Loudness and [`Pipe`] are orthogonal: one controls
//! whether a line shows, the other controls which stream it shows on. Styling
//! comes from the process-wide [`PALETTE`].
//!
//! ```
//! use rawr_output::{Line, Piece, Render, PALETTE};
//! let line = Line::new([
//!     Piece::fixed("hello", &PALETTE.heading),
//!     Piece::space(),
//!     Piece::plain("world"),
//! ]);
//! // Without colour, styling is dropped and pieces render verbatim.
//! assert_eq!(&*line.render(None, false), "hello world");
//! ```
//!
//! A typical caller implements [`IntoLines`] for a domain type, then prints each
//! line:
//!
//! ```ignore
//! let output = Arc::new(PrintingOutput::new(cli.color, cli.verbose, cli.quiet));
//! for line in ctx.to_lines() {
//!     output.print(Pipe::Out, &line);
//! }
//! ```
//!
//! That example is `ignore`d because building a [`PrintingOutput`] needs CLI
//! flags and a real terminal, which a doctest cannot supply.

mod error;
mod render;
mod sink;
mod style;

pub use self::error::{Error, Result};
pub use self::render::line::{Line, Loudness};
pub use self::render::piece::Piece;
pub use self::render::{IntoLines, Render};
pub use self::sink::{Output, PrintingOutput};
pub use self::style::PALETTE;

/// Which stream a line is written to.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Pipe {
    /// Standard output.
    #[default]
    Out,
    /// Standard error.
    Err,
}

/// The active output level, set once from the CLI flags.
///
/// The variants order as `Quiet < Normal < Verbose`, so the derived [`Ord`]
/// compares levels directly. A line shows only when its [`Loudness`] passes
/// [`Loudness::is_visible`] at the active level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}
