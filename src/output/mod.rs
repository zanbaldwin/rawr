mod line;
mod piece;
pub mod sink;
mod style;
pub mod util;

pub use self::line::{Line, Loudness};
pub use self::piece::{Flexibility, Piece};
pub use self::style::Palette;
use indicatif::ProgressBar;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pipe {
    Out,
    Err,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

pub trait Render<'a> {
    fn render(&'a self, width: Option<usize>, colour: bool) -> Cow<'a, str>;
}

/// Output abstraction for CLI rendering.
///
/// `Pipe` (stdout vs stderr) and `Loudness` (verbosity filter) are orthogonal
/// concerns, combined via the `Line` builder passed to `print()`.
pub trait Output {
    /// Render a line to the appropriate stream, filtered by loudness.
    fn print(&self, pipe: Pipe, line: &Line<'_>);

    /// Create a spinner for indeterminate progress. Returns hidden bar in quiet mode.
    fn spinner(&self, message: &str) -> ProgressBar;

    /// Create a progress bar for determinate progress. Returns hidden bar in quiet mode.
    fn progress_bar(&self, label: &str) -> ProgressBar;

    /// Yes/no confirmation prompt. Returns false if not interactive.
    // TODO: Replace Result with crate's error Result type.
    fn confirm(&self, prompt: &str) -> Result<bool, ()>;
}

struct PerPipe<T> {
    out: T,
    err: T,
}
impl<T> PerPipe<T> {
    fn new(out: T, err: T) -> Self {
        Self { out, err }
    }

    fn get(&self, pipe: Pipe) -> &T {
        match pipe {
            Pipe::Out => &self.out,
            Pipe::Err => &self.err,
        }
    }
}
