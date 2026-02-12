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
    /// Returns true if the work is complete (planned chapters have been written).
    pub fn is_complete(&self) -> bool {
        self.total.map(|t| self.written >= t).unwrap_or(false)
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
