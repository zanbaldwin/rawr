//! Buffering Output
//!
//! In-memory output backend for tests. Captures rendered lines per pipe
//! so assertions can verify what the CLI would have printed.

use super::{CursorGuard, Output, PerPipe};
use crate::error::Result;
use crate::output::{Line, Render};
use crate::output::{Pipe, Verbosity};
use console::Term;
use indicatif::ProgressBar;
use std::cell::RefCell;
use std::collections::VecDeque;

pub(crate) struct BufferingOutput {
    lines: PerPipe<RefCell<Vec<String>>>,
    verbosity: Verbosity,
    width: Option<usize>,
    colour: bool,
    confirm_responses: RefCell<VecDeque<bool>>,
    confirm_prompts: RefCell<Vec<String>>,
}

impl BufferingOutput {
    pub(crate) fn new() -> Self {
        Self {
            lines: PerPipe::new(RefCell::new(Vec::new()), RefCell::new(Vec::new())),
            verbosity: Verbosity::Normal,
            width: Some(120),
            colour: false,
            confirm_responses: RefCell::new(VecDeque::new()),
            confirm_prompts: RefCell::new(Vec::new()),
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

    pub(crate) fn push_confirm_response(&self, response: bool) {
        self.confirm_responses.borrow_mut().push_back(response);
    }

    pub(crate) fn lines(&self, pipe: Pipe) -> Vec<String> {
        self.lines.get(pipe).borrow().clone()
    }

    pub(crate) fn line_count(&self, pipe: Pipe) -> usize {
        self.lines.get(pipe).borrow().len()
    }

    pub(crate) fn contains(&self, pipe: Pipe, needle: &str) -> bool {
        self.lines.get(pipe).borrow().iter().any(|l| l.contains(needle))
    }

    pub(crate) fn confirm_prompts(&self) -> Vec<String> {
        self.confirm_prompts.borrow().clone()
    }
}

impl Output for BufferingOutput {
    fn print(&self, pipe: Pipe, line: &Line<'_>) {
        if !line.is_visible(self.verbosity) {
            return;
        }
        let rendered = line.render(self.width, self.colour);
        self.lines.get(pipe).borrow_mut().push(rendered.into_owned());
    }

    fn spinner(&self, _message: &str) -> ProgressBar {
        ProgressBar::hidden()
    }

    fn progress_bar(&self, _label: &str) -> ProgressBar {
        ProgressBar::hidden()
    }

    fn confirm(&self, prompt: &str) -> Result<bool> {
        self.confirm_prompts.borrow_mut().push(prompt.to_string());
        Ok(self.confirm_responses.borrow_mut().pop_front().unwrap_or(false))
    }

    fn is_interactive(&self, _pipe: Pipe) -> bool {
        false
    }

    fn alt(&self, _pipe: Pipe) -> Result<(CursorGuard<'_>, &Term)> {
        Err(miette::miette!("Cannot enter alternative screen; buffering output is not an interactive terminal"))?
    }
}
