#[cfg(test)]
mod buffer;
mod print;

#[cfg(test)]
pub(crate) use self::buffer::BufferingOutput;
pub use self::print::PrintingOutput;
use crate::error::Result;
use crate::{Line, Pipe};
use console::Term;
#[cfg(feature = "progress")]
use indicatif::ProgressBar;

/// Where rendered [`Line`]s go, with verbosity filtering applied per line.
///
/// Held as `Arc<dyn Output>`, hence the `Send + Sync` bound. The chosen [`Pipe`]
/// and a line's [`Loudness`](crate::Loudness) are orthogonal: the pipe picks the
/// stream, the loudness decides whether the line survives the active
/// [`Verbosity`](crate::Verbosity).
pub trait Output: Send + Sync {
    /// Render `line` to the stream named by `pipe`, ignoring the line's
    /// [`pipe_hint`](crate::Line::pipe_hint), dropping it if its
    /// [`Loudness`](crate::Loudness) is not visible at the active
    /// [`Verbosity`](crate::Verbosity).
    fn print_to(&self, pipe: Pipe, line: &Line<'_>);

    /// Spinner for indeterminate progress; hidden in quiet mode.
    #[cfg(feature = "progress")]
    fn spinner(&self, message: &str) -> ProgressBar;

    /// Progress bar for determinate progress; hidden in quiet mode.
    #[cfg(feature = "progress")]
    fn progress_bar(&self, label: &str) -> ProgressBar;

    /// Yes/no confirmation prompt on stderr, returning `false` when stderr is
    /// not interactive.
    #[cfg(feature = "confirm")]
    fn confirm(&self, prompt: &str) -> Result<bool>;

    /// Whether `pipe` is an interactive terminal rather than a file or another
    /// pipe.
    fn is_interactive(&self, pipe: Pipe) -> bool;

    /// Enter a pseudo alt-screen on `pipe`, returning a `CursorGuard` and the
    /// underlying terminal.
    ///
    /// This is not a real secondary screen (as Ratatui uses); it only manages
    /// the cursor for in-place updates on the current screen.
    fn alt(&self, pipe: Pipe) -> Result<(CursorGuard<'_>, &Term)>;
}

/// Ergonomic [`Output`] helpers; bring into scope with `use rawr_output::OutputExt`.
///
/// Implemented for every [`Output`] (including `dyn Output`) via a blanket impl,
/// so callers keep the convenience surface without the backend trait having to
/// carry generic, non-dyn-compatible methods.
pub trait OutputExt: Output {
    /// Render `line`, choosing the stream from `pipe`: pass a [`Pipe`], `None`,
    /// or `Some(Pipe)`. `None` falls back to the line's
    /// [`pipe_hint`](crate::Line::pipe_hint).
    fn print(&self, pipe: impl Into<Option<Pipe>>, line: &Line<'_>) {
        self.print_to(pipe.into().unwrap_or(line.pipe_hint), line);
    }
}
impl<T: Output + ?Sized> OutputExt for T {}

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

/// RAII guard returned by [`Output::alt`]. With the `progress` feature it hides
/// the cursor on creation and restores it on drop.
pub struct CursorGuard<'a>(&'a Term);
impl<'a> CursorGuard<'a> {
    pub(crate) fn new(term: &'a Term) -> Self {
        #[cfg(feature = "progress")]
        {
            _ = term.hide_cursor();
        }
        Self(term)
    }
}
impl Drop for CursorGuard<'_> {
    fn drop(&mut self) {
        #[cfg(feature = "progress")]
        {
            _ = self.0.show_cursor();
        }
    }
}
