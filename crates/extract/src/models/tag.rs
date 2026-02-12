use std::fmt::{Display, Formatter, Result as FmtResult};

/// A tag applied to a work.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tag {
    /// Tag text
    pub name: String,
    /// Type of tag
    pub kind: TagKind,
}

/// Tag type enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagKind {
    /// Relationship tags (e.g., "Character A/Character B")
    Relationship,
    /// Character tags
    Character,
    /// Freeform/additional tags
    Freeform,
}
impl TagKind {
    /// Returns the display string for the tag kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            TagKind::Relationship => "Relationship",
            TagKind::Character => "Character",
            TagKind::Freeform => "Freeform",
        }
    }
}
impl Display for TagKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}
