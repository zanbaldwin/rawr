use super::{CursorGuard, Output, PerPipe};
use crate::error::{Error, Result};
use crate::{Line, Pipe, Render, Verbosity};
use console::Term;
#[cfg(feature = "confirm")]
use dialoguer::Confirm;
#[cfg(feature = "progress")]
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

struct Terminal {
    pipe: Term,
    width: Option<usize>,
    colors: bool,
}

pub struct PrintingOutput {
    terminal: PerPipe<Terminal>,
    verbosity: Verbosity,
    #[cfg(feature = "progress")]
    multi: MultiProgress,
}
impl PrintingOutput {
    pub fn new(color: clap::ColorChoice, verbose: bool, quiet: bool) -> Self {
        let stdout = Term::stdout();
        let stderr = Term::stderr();
        let verbosity = match (quiet, verbose) {
            (true, _) => Verbosity::Quiet,
            (_, true) => Verbosity::Verbose,
            _ => Verbosity::Normal,
        };
        let out = Terminal {
            width: Self::get_term_width(&stdout),
            colors: Self::term_has_colour(&stdout, &color),
            pipe: stdout,
        };
        let err = Terminal {
            width: Self::get_term_width(&stderr),
            colors: Self::term_has_colour(&stderr, &color),
            pipe: stderr,
        };
        Self {
            terminal: PerPipe::new(out, err),
            verbosity,
            #[cfg(feature = "progress")]
            multi: MultiProgress::new(),
        }
    }

    fn get_term_width(term: &Term) -> Option<usize> {
        if term.is_term() {
            match term.size() {
                (_, 0) => None,
                (_, w) => Some(w as usize),
            }
        } else {
            None
        }
    }

    pub(crate) fn term_has_colour(term: &Term, choice: &clap::ColorChoice) -> bool {
        match choice {
            clap::ColorChoice::Always => true,
            clap::ColorChoice::Never => false,
            clap::ColorChoice::Auto => term.is_term() && term.features().colors_supported(),
        }
    }
}

impl Output for PrintingOutput {
    fn print(&self, pipe: Pipe, line: &Line<'_>) {
        if !line.is_visible(self.verbosity) {
            return;
        }
        let terminal = self.terminal.get(pipe);
        let rendered = line.render(terminal.width, terminal.colors);
        #[cfg(feature = "progress")]
        {
            _ = self.multi.suspend(|| terminal.pipe.write_line(&rendered));
        }
        #[cfg(not(feature = "progress"))]
        {
            _ = terminal.pipe.write_line(&rendered);
        }
    }

    #[cfg(feature = "progress")]
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

    #[cfg(feature = "progress")]
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

    #[cfg(feature = "confirm")]
    fn confirm(&self, prompt: &str) -> Result<bool> {
        let stderr = &self.terminal.get(Pipe::Err).pipe;
        if !stderr.is_term() {
            return Ok(false);
        }
        Ok(Confirm::new().with_prompt(prompt).default(false).interact_on(stderr)?)
    }

    fn is_interactive(&self, pipe: Pipe) -> bool {
        self.terminal.get(pipe).pipe.is_term()
    }

    fn alt(&self, pipe: Pipe) -> Result<(CursorGuard<'_>, &Term)> {
        if !self.is_interactive(pipe) {
            return Err(Error::AltScreenUnavailable("pipe"));
        }
        let term = &self.terminal.get(pipe).pipe;
        Ok((CursorGuard::new(term), term))
    }
}
