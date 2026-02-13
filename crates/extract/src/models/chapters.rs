use std::fmt::{Display, Formatter, Result as FmtResult};

/// Chapter count information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chapters {
    /// Number of chapters currently posted
    pub written: u32,
    /// Expected total chapters (None if unknown/`?`)
    pub total: Option<u32>,
}
impl Chapters {
    pub fn new(written: u32, total: Option<u32>) -> Self {
        Self { written, total }
    }
    /// Returns true if the work is complete (planned chapters have been written).
    pub fn is_complete(&self) -> bool {
        self.total.is_some_and(|t| self.written >= t)
    }
}
impl From<(u32, u32)> for Chapters {
    fn from((written, total): (u32, u32)) -> Self {
        Chapters::new(written, Some(total))
    }
}
impl From<(u32, Option<u32>)> for Chapters {
    fn from((written, total): (u32, Option<u32>)) -> Self {
        Chapters::new(written, total)
    }
}
impl Display for Chapters {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self.total {
            Some(total) => write!(f, "{}/{total}", self.written),
            None => write!(f, "{}/?", self.written),
        }
    }
}
