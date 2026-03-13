#[cfg(test)]
mod buffer;
mod print;

#[cfg(test)]
pub(crate) use self::buffer::BufferingOutput;
pub use self::print::PrintingOutput;
use crate::error::Result;
use crate::output::{Line, Pipe};
use indicatif::ProgressBar;

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
    fn confirm(&self, prompt: &str) -> Result<bool>;
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
