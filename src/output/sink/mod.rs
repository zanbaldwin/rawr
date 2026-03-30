#[cfg(test)]
mod buffer;
mod print;

#[cfg(test)]
pub(crate) use self::buffer::BufferingOutput;
pub use self::print::PrintingOutput;
use crate::error::Result;
use crate::output::{Line, Pipe};
use console::Term;
use indicatif::ProgressBar;

/// Output abstraction for CLI rendering.
///
/// `Pipe` (stdout vs stderr) and `Loudness` (verbosity filter) are orthogonal
/// concerns, combined via the `Line` builder passed to `print()`.
pub trait Output: Send + Sync {
    /// Render a line to the appropriate stream, filtered by loudness.
    fn print(&self, pipe: Pipe, line: &Line<'_>);

    /// Create a spinner for indeterminate progress. Returns hidden bar in quiet mode.
    fn spinner(&self, message: &str) -> ProgressBar;

    /// Create a progress bar for determinate progress. Returns hidden bar in quiet mode.
    fn progress_bar(&self, label: &str) -> ProgressBar;

    /// Yes/no confirmation prompt. Returns false if not interactive.
    fn confirm(&self, prompt: &str) -> Result<bool>;

    fn is_interactive(&self, pipe: Pipe) -> bool;

    /// Pseudo-alt screen for interactive output (but not the true
    /// secondary/alt terminal screen like Ratatui uses).
    fn alt(&self, pipe: Pipe) -> Result<(CursorGuard<'_>, &Term)>;
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

pub(crate) struct CursorGuard<'a>(&'a Term);
impl<'a> CursorGuard<'a> {
    pub(crate) fn new(term: &'a Term) -> Self {
        _ = term.hide_cursor();
        Self(term)
    }
}
impl Drop for CursorGuard<'_> {
    fn drop(&mut self) {
        _ = self.0.show_cursor();
    }
}
