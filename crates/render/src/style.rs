use crate::error::Result;
use std::path::PathBuf;

enum Style {
    Builtin(String),
    UserPath(PathBuf),
    UserContent(String),
}

#[derive(Default)]
pub struct StyleConfig {
    styles: Vec<Style>,
}
impl StyleConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtin(mut self, name: impl Into<String>) -> Result<Self> {
        todo!()
    }

    pub fn with_file(mut self, path: impl Into<PathBuf>) -> Result<Self> {
        todo!()
    }

    pub fn with_content(mut self, content: impl Into<String>) -> Result<Self> {
        todo!()
    }
}
