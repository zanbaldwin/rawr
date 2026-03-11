mod line;
mod style;
#[cfg(test)]
mod test;

pub(crate) use self::line::{Line, Loudness, Piece, Pipe};
pub(crate) use self::style::Palette;
#[cfg(test)]
pub(crate) use self::test::TestOutput;
use console::Term;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

/// Output abstraction for CLI rendering.
///
/// `Pipe` (stdout vs stderr) and `Loudness` (verbosity filter) are orthogonal
/// concerns, combined via the `Line` builder passed to `print()`.
pub(crate) trait Output {
    /// Render a line to the appropriate stream, filtered by loudness.
    fn print(&self, line: &Line<'_>);

    /// Create a spinner for indeterminate progress. Returns hidden bar in quiet mode.
    fn spinner(&self, message: &str) -> ProgressBar;

    /// Create a progress bar for determinate progress. Returns hidden bar in quiet mode.
    fn progress_bar(&self, label: &str) -> ProgressBar;

    /// Yes/no confirmation prompt. Returns false if not interactive.
    fn confirm(&self, prompt: &str) -> std::io::Result<bool>;
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

struct Terminal {
    pipe: Term,
    width: Option<u16>,
    colors: bool,
}

pub(crate) struct TerminalOutput {
    terminal: PerPipe<Terminal>,
    verbosity: Verbosity,
    multi: MultiProgress,
}

impl TerminalOutput {
    pub fn new(color: clap::ColorChoice, verbose: bool, quiet: bool) -> Self {
        let stdout = Term::stdout();
        let stderr = Term::stderr();
        let verbosity = match (quiet, verbose) {
            (true, _) => Verbosity::Quiet,
            (_, true) => Verbosity::Verbose,
            _ => Verbosity::Normal,
        };

        let has_color = |term: &Term, choice: &clap::ColorChoice| -> bool {
            match choice {
                clap::ColorChoice::Always => true,
                clap::ColorChoice::Never => false,
                clap::ColorChoice::Auto => term.is_term() && term.features().colors_supported(),
            }
        };

        let term_width = |term: &Term| -> Option<u16> {
            if term.is_term() {
                match term.size() {
                    (_, 0) => None,
                    (_, w) => Some(w),
                }
            } else {
                None
            }
        };

        let out = Terminal {
            width: term_width(&stdout),
            colors: has_color(&stdout, &color),
            pipe: stdout,
        };
        let err = Terminal {
            width: term_width(&stderr),
            colors: has_color(&stderr, &color),
            pipe: stderr,
        };

        Self {
            terminal: PerPipe::new(out, err),
            verbosity,
            multi: MultiProgress::new(),
        }
    }
}

impl Output for TerminalOutput {
    fn print(&self, line: &Line<'_>) {
        if !line.loudness.should_print(&self.verbosity) {
            return;
        }
        let terminal = self.terminal.get(line.pipe);
        let rendered = line.render(terminal.width, terminal.colors);
        _ = terminal.pipe.write_line(&rendered);
    }

    fn spinner(&self, message: &str) -> ProgressBar {
        if self.verbosity == Verbosity::Quiet {
            return ProgressBar::hidden();
        }
        let bar = self.multi.add(ProgressBar::new_spinner());
        bar.enable_steady_tick(std::time::Duration::from_millis(100));
        bar.set_style(
            ProgressStyle::default_spinner().template("{spinner:.cyan} {msg} [{elapsed_precise}] {pos}").unwrap(),
        );
        bar.set_message(message.to_string());
        bar
    }

    fn progress_bar(&self, label: &str) -> ProgressBar {
        if self.verbosity == Verbosity::Quiet {
            return ProgressBar::hidden();
        }
        let bar = self.multi.add(ProgressBar::new(0));
        bar.enable_steady_tick(std::time::Duration::from_millis(100));
        bar.set_style(
            ProgressStyle::default_bar()
                .template(&format!(
                    "{{spinner:.green}} {label} [{{bar:40.cyan/blue}}] \
                     {{pos}}/{{len}} ({{percent}}%) [{{elapsed_precise}}]"
                ))
                .unwrap(),
        );
        bar
    }

    fn confirm(&self, prompt: &str) -> std::io::Result<bool> {
        let stderr = &self.terminal.get(Pipe::Err).pipe;
        if !stderr.is_term() {
            return Ok(false);
        }
        Ok(dialoguer::Confirm::new().with_prompt(prompt).default(false).interact_on(stderr)?)
    }
}
