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
