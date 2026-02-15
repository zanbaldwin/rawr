use super::sanitize;
use crate::error::{Error, ErrorKind};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::str::FromStr;

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
impl FromStr for TagKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let sanitized = sanitize(s);
        Ok(match sanitized.as_str() {
            "relationship" => Self::Relationship,
            "character" => Self::Character,
            "freeform" => Self::Freeform,
            _ => exn::bail!(ErrorKind::ParseError { field: "tag_kind", value: s.to_string() }),
        })
    }
}
