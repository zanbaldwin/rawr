use super::sanitize;
use crate::error::{Error, ErrorKind};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::str::FromStr;

/// A tag applied to a work.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Tag {
    /// Tag text
    pub name: String,
    /// Type of tag
    pub kind: TagKind,
}

/// Tag type enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde_json::{from_str as from_json, to_string as to_json};

    #[rstest]
    #[case(Tag{name: "Draco/Harry".to_string(), kind: TagKind::Relationship}, r#"{"name":"Draco/Harry","kind":"Relationship"}"#)]
    #[case(Tag{name: "Draco Malfoy".to_string(), kind: TagKind::Character}, r#"{"name":"Draco Malfoy","kind":"Character"}"#)]
    #[case(Tag{name: "Fluff".to_string(), kind: TagKind::Freeform}, r#"{"name":"Fluff","kind":"Freeform"}"#)]
    fn test_tag_serialize(#[case] input: Tag, #[case] expected: impl AsRef<str>) {
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), expected.as_ref());
    }

    #[rstest]
    #[case(Tag{name: "Draco/Harry".to_string(), kind: TagKind::Relationship}, r#"{"name":"Draco/Harry","kind":"Relationship"}"#)]
    #[case(Tag{name: "Draco Malfoy".to_string(), kind: TagKind::Character}, r#"{"name":"Draco Malfoy","kind":"Character"}"#)]
    #[case(Tag{name: "Fluff".to_string(), kind: TagKind::Freeform}, r#"{"name":"Fluff","kind":"Freeform"}"#)]
    fn test_tag_deserialize(#[case] expected: Tag, #[case] input: impl AsRef<str>) {
        let obj = from_json::<Tag>(input.as_ref()).unwrap();
        assert_eq!(obj, expected);
    }

    #[test]
    fn test_tag_vec_serialize() {
        let input = vec![
            Tag {
                name: "Draco/Harry".to_string(),
                kind: TagKind::Relationship,
            },
            Tag {
                name: "Fluff".to_string(),
                kind: TagKind::Freeform,
            },
        ];
        let json = to_json(&input).unwrap();
        assert_eq!(
            json.as_str(),
            r#"[{"name":"Draco/Harry","kind":"Relationship"},{"name":"Fluff","kind":"Freeform"}]"#
        );
    }

    #[test]
    fn test_tag_vec_deserialize() {
        let expected = vec![
            Tag {
                name: "Draco/Harry".to_string(),
                kind: TagKind::Relationship,
            },
            Tag {
                name: "Fluff".to_string(),
                kind: TagKind::Freeform,
            },
        ];
        let obj = from_json::<Vec<Tag>>(
            r#"[{"name":"Draco/Harry","kind":"Relationship"},{"name":"Fluff","kind":"Freeform"}]"#,
        )
        .unwrap();
        assert_eq!(obj, expected);
    }
}
