//! Buffering Output
//!
//! In-memory output backend for tests. Captures rendered lines per pipe so
//! assertions can verify what the CLI would have printed.

use super::{CursorGuard, Output, PerPipe};
use crate::error::{Error, Result};
use crate::{Line, Render};
use crate::{Pipe, Verbosity};
use console::Term;
#[cfg(feature = "progress")]
use indicatif::ProgressBar;
use std::collections::VecDeque;
use std::sync::Mutex;

/// In-memory [`Output`](crate::Output) backend for tests, capturing rendered
/// lines per [`Pipe`](crate::Pipe).
pub(crate) struct BufferingOutput {
    lines: PerPipe<Mutex<Vec<String>>>,
    verbosity: Verbosity,
    width: Option<usize>,
    colour: bool,
    confirm_responses: Mutex<VecDeque<bool>>,
    confirm_prompts: Mutex<Vec<String>>,
}

impl BufferingOutput {
    pub(crate) fn new() -> Self {
        Self {
            lines: PerPipe::new(Mutex::new(Vec::new()), Mutex::new(Vec::new())),
            verbosity: Verbosity::Normal,
            width: Some(120),
            colour: false,
            confirm_responses: Mutex::new(VecDeque::new()),
            confirm_prompts: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn with_verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
        self
    }

    pub(crate) fn with_width(mut self, width: Option<usize>) -> Self {
        self.width = width;
        self
    }

    pub(crate) fn with_colour(mut self, colour: bool) -> Self {
        self.colour = colour;
        self
    }

    /// Queues a reply for a later [`confirm`](Self::confirm) call; replies are
    /// consumed in order, defaulting to `false` once drained.
    pub(crate) fn push_confirm_response(&self, response: bool) {
        self.confirm_responses.lock().unwrap().push_back(response);
    }

    pub(crate) fn lines(&self, pipe: Pipe) -> Vec<String> {
        self.lines.get(pipe).lock().unwrap().clone()
    }

    pub(crate) fn line_count(&self, pipe: Pipe) -> usize {
        self.lines.get(pipe).lock().unwrap().len()
    }

    pub(crate) fn contains(&self, pipe: Pipe, needle: &str) -> bool {
        self.lines.get(pipe).lock().unwrap().iter().any(|l| l.contains(needle))
    }

    pub(crate) fn confirm_prompts(&self) -> Vec<String> {
        self.confirm_prompts.lock().unwrap().clone()
    }
}

impl Output for BufferingOutput {
    fn print(&self, pipe: Pipe, line: &Line<'_>) {
        if !line.is_visible(self.verbosity) {
            return;
        }
        let rendered = line.render(self.width, self.colour);
        self.lines.get(pipe).lock().unwrap().push(rendered.into_owned());
    }

    #[cfg(feature = "progress")]
    fn spinner(&self, _message: &str) -> ProgressBar {
        ProgressBar::hidden()
    }

    #[cfg(feature = "progress")]
    fn progress_bar(&self, _label: &str) -> ProgressBar {
        ProgressBar::hidden()
    }

    fn confirm(&self, prompt: &str) -> Result<bool> {
        self.confirm_prompts.lock().unwrap().push(prompt.to_string());
        Ok(self.confirm_responses.lock().unwrap().pop_front().unwrap_or(false))
    }

    fn is_interactive(&self, _pipe: Pipe) -> bool {
        false
    }

    fn alt(&self, _pipe: Pipe) -> Result<(CursorGuard<'_>, &Term)> {
        Err(Error::AltScreenUnavailable("buffering output"))
    }
}
