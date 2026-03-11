use std::cell::RefCell;

use indicatif::ProgressBar;

use crate::output::Pipe;

use super::{Line, Output};

pub(crate) struct TestOutput {
    results: RefCell<Vec<String>>,
    messages: RefCell<Vec<String>>,
}

impl TestOutput {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            results: RefCell::new(Vec::new()),
            messages: RefCell::new(Vec::new()),
        }
    }

    #[allow(dead_code)]
    pub fn results(&self) -> Vec<String> {
        self.results.borrow().clone()
    }

    #[allow(dead_code)]
    pub fn messages(&self) -> Vec<String> {
        self.messages.borrow().clone()
    }
}

impl Output for TestOutput {
    fn print(&self, line: &Line<'_>) {
        let rendered = line.render(Some(120), false);
        match line.pipe {
            Pipe::Out => self.results.borrow_mut().push(rendered),
            Pipe::Err => self.messages.borrow_mut().push(rendered),
        }
    }

    fn spinner(&self, _message: &str) -> ProgressBar {
        ProgressBar::hidden()
    }

    fn progress_bar(&self, _label: &str) -> ProgressBar {
        ProgressBar::hidden()
    }

    fn confirm(&self, _prompt: &str) -> std::io::Result<bool> {
        Ok(false)
    }
}
