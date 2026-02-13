use std::fmt::{Display, Formatter, Result as FmtResult};

/// A work's position within a series.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SeriesPosition {
    /// AO3 Series ID
    pub id: u64,
    /// Series name
    pub name: String,
    /// Position in series (1-indexed)
    pub position: u32,
}
impl SeriesPosition {
    pub fn new(id: u64, name: impl Into<String>, position: u32) -> Self {
        Self { id, name: name.into(), position }
    }
}
impl Display for SeriesPosition {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "Part {} of \"{}\"", self.position, self.name)
    }
}
