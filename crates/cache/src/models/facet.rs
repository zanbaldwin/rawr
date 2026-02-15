use rawr_extract::models as extract;

#[derive(facet::Facet)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct AuthorProxy {
    #[facet(rename = "u")]
    username: String,
    #[facet(rename = "p", default, transparent, skip_serializing_if = Option::is_none)]
    pseudonym: Option<String>,
}
impl From<&extract::Author> for AuthorProxy {
    fn from(author: &extract::Author) -> Self {
        Self {
            username: author.username.clone(),
            pseudonym: author.pseudonym.clone(),
        }
    }
}
impl From<AuthorProxy> for extract::Author {
    fn from(author: AuthorProxy) -> Self {
        Self {
            username: author.username,
            pseudonym: author.pseudonym,
        }
    }
}

#[derive(facet::Facet)]
#[cfg_attr(test, derive(Debug, PartialEq))]
#[facet(transparent)]
pub(crate) struct FandomProxy(String);
impl From<&extract::Fandom> for FandomProxy {
    fn from(fandom: &extract::Fandom) -> Self {
        Self(fandom.name.clone())
    }
}
impl From<FandomProxy> for extract::Fandom {
    fn from(fandom: FandomProxy) -> Self {
        Self { name: fandom.0 }
    }
}

#[derive(facet::Facet)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct SeriesPositionProxy {
    pub id: u64,
    pub name: String,
    #[facet(rename = "pos")]
    pub position: u32,
}
impl From<&extract::SeriesPosition> for SeriesPositionProxy {
    fn from(series: &extract::SeriesPosition) -> Self {
        Self {
            id: series.id,
            name: series.name.clone(),
            position: series.position,
        }
    }
}
impl From<SeriesPositionProxy> for extract::SeriesPosition {
    fn from(series: SeriesPositionProxy) -> Self {
        Self {
            id: series.id,
            name: series.name,
            position: series.position,
        }
    }
}

#[derive(facet::Facet)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct TagProxy {
    #[facet(rename = "n")]
    name: String,
    #[facet(rename = "k")]
    kind: TagKindProxy,
}
impl From<&extract::Tag> for TagProxy {
    fn from(tag: &extract::Tag) -> Self {
        Self {
            name: tag.name.clone(),
            kind: (&tag.kind).into(),
        }
    }
}
impl From<TagProxy> for extract::Tag {
    fn from(tag: TagProxy) -> Self {
        Self { name: tag.name, kind: tag.kind.into() }
    }
}

#[repr(u8)]
#[derive(facet::Facet)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) enum TagKindProxy {
    R,
    C,
    F,
}
impl From<&extract::TagKind> for TagKindProxy {
    fn from(kind: &extract::TagKind) -> Self {
        match kind {
            extract::TagKind::Relationship => Self::R,
            extract::TagKind::Character => Self::C,
            extract::TagKind::Freeform => Self::F,
        }
    }
}
impl From<TagKindProxy> for extract::TagKind {
    fn from(kind: TagKindProxy) -> Self {
        match kind {
            TagKindProxy::R => Self::Relationship,
            TagKindProxy::C => Self::Character,
            TagKindProxy::F => Self::Freeform,
        }
    }
}

#[repr(u8)]
#[derive(facet::Facet)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) enum WarningProxy {
    NoWarningsApply,
    CreatorChoseNotToUse,
    GraphicViolence,
    MajorCharacterDeath,
    Underage,
    NonCon,
}
impl From<&extract::Warning> for WarningProxy {
    fn from(warning: &extract::Warning) -> Self {
        match warning {
            extract::Warning::NoWarningsApply => Self::NoWarningsApply,
            extract::Warning::CreatorChoseNotToUse => Self::CreatorChoseNotToUse,
            extract::Warning::GraphicViolence => Self::GraphicViolence,
            extract::Warning::MajorCharacterDeath => Self::MajorCharacterDeath,
            extract::Warning::Underage => Self::Underage,
            extract::Warning::NonCon => Self::NonCon,
        }
    }
}
impl From<WarningProxy> for extract::Warning {
    fn from(warning: WarningProxy) -> Self {
        match warning {
            WarningProxy::NoWarningsApply => Self::NoWarningsApply,
            WarningProxy::CreatorChoseNotToUse => Self::CreatorChoseNotToUse,
            WarningProxy::GraphicViolence => Self::GraphicViolence,
            WarningProxy::MajorCharacterDeath => Self::MajorCharacterDeath,
            WarningProxy::Underage => Self::Underage,
            WarningProxy::NonCon => Self::NonCon,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_json::{from_str as from_json, to_string as to_json};
    use rstest::rstest;

    #[rstest]
    #[case(AuthorProxy{username: "user123".to_string(), pseudonym: None}, r#"{"u":"user123"}"#)]
    #[case(AuthorProxy{username: "user321".to_string(), pseudonym: Some("another1".to_string())}, r#"{"u":"user321","p":"another1"}"#)]
    fn test_author_serialize(#[case] input: AuthorProxy, #[case] expected: impl AsRef<str>) {
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), expected.as_ref());
    }

    #[rstest]
    #[case(AuthorProxy{username: "user123".to_string(), pseudonym: None}, r#"{"u":"user123"}"#)]
    #[case(AuthorProxy{username: "user321".to_string(), pseudonym: Some("another1".to_string())}, r#"{"u":"user321","p":"another1"}"#)]
    fn test_author_deserialize(#[case] expected: AuthorProxy, #[case] input: impl AsRef<str>) {
        let obj = from_json::<AuthorProxy>(input.as_ref()).unwrap();
        assert_eq!(obj, expected);
    }

    #[rstest]
    #[case(FandomProxy("Harry Potter".to_string()), r#""Harry Potter""#)]
    fn test_fandom_serialize(#[case] input: FandomProxy, #[case] expected: impl AsRef<str>) {
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), expected.as_ref());
    }

    #[rstest]
    #[case(FandomProxy("Harry Potter".to_string()), r#""Harry Potter""#)]
    fn test_fandom_deserialize(#[case] expected: FandomProxy, #[case] input: impl AsRef<str>) {
        let obj = from_json::<FandomProxy>(input.as_ref()).unwrap();
        assert_eq!(obj, expected);
    }

    #[rstest]
    #[case(SeriesPositionProxy{id: 123, name: "Series".to_string(), position: 5}, r#"{"id":123,"name":"Series","pos":5}"#)]
    fn test_series_position_serialize(#[case] input: SeriesPositionProxy, #[case] expected: impl AsRef<str>) {
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), expected.as_ref());
    }

    #[rstest]
    #[case(SeriesPositionProxy{id: 123, name: "Series".to_string(), position: 5}, r#"{"id":123,"name":"Series","pos":5}"#)]
    fn test_series_position_deserialize(#[case] expected: SeriesPositionProxy, #[case] input: impl AsRef<str>) {
        let obj = from_json::<SeriesPositionProxy>(input.as_ref()).unwrap();
        assert_eq!(obj, expected);
    }

    #[rstest]
    #[case(TagProxy{name: "Draco/Harry".to_string(), kind: TagKindProxy::R}, r#"{"n":"Draco/Harry","k":"R"}"#)]
    #[case(TagProxy{name: "Draco Malfoy".to_string(), kind: TagKindProxy::C}, r#"{"n":"Draco Malfoy","k":"C"}"#)]
    #[case(TagProxy{name: "Fluff".to_string(), kind: TagKindProxy::F}, r#"{"n":"Fluff","k":"F"}"#)]
    fn test_tag_serialize(#[case] input: TagProxy, #[case] expected: impl AsRef<str>) {
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), expected.as_ref());
    }

    #[rstest]
    #[case(TagProxy{name: "Draco/Harry".to_string(), kind: TagKindProxy::R}, r#"{"n":"Draco/Harry","k":"R"}"#)]
    #[case(TagProxy{name: "Draco Malfoy".to_string(), kind: TagKindProxy::C}, r#"{"n":"Draco Malfoy","k":"C"}"#)]
    #[case(TagProxy{name: "Fluff".to_string(), kind: TagKindProxy::F}, r#"{"n":"Fluff","k":"F"}"#)]
    fn test_tag_deserialize(#[case] expected: TagProxy, #[case] input: impl AsRef<str>) {
        let obj = from_json::<TagProxy>(input.as_ref()).unwrap();
        assert_eq!(obj, expected);
    }

    #[rstest]
    #[case(WarningProxy::NoWarningsApply, r#""NoWarningsApply""#)]
    #[case(WarningProxy::CreatorChoseNotToUse, r#""CreatorChoseNotToUse""#)]
    #[case(WarningProxy::GraphicViolence, r#""GraphicViolence""#)]
    #[case(WarningProxy::MajorCharacterDeath, r#""MajorCharacterDeath""#)]
    #[case(WarningProxy::Underage, r#""Underage""#)]
    #[case(WarningProxy::NonCon, r#""NonCon""#)]
    fn test_warning_serialize(#[case] input: WarningProxy, #[case] expected: impl AsRef<str>) {
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), expected.as_ref());
    }

    #[rstest]
    #[case(WarningProxy::NoWarningsApply, r#""NoWarningsApply""#)]
    #[case(WarningProxy::CreatorChoseNotToUse, r#""CreatorChoseNotToUse""#)]
    #[case(WarningProxy::GraphicViolence, r#""GraphicViolence""#)]
    #[case(WarningProxy::MajorCharacterDeath, r#""MajorCharacterDeath""#)]
    #[case(WarningProxy::Underage, r#""Underage""#)]
    #[case(WarningProxy::NonCon, r#""NonCon""#)]
    fn test_warning_deserialize(#[case] expected: WarningProxy, #[case] input: impl AsRef<str>) {
        let obj = from_json::<WarningProxy>(input.as_ref()).unwrap();
        assert_eq!(obj, expected);
    }

    #[test]
    fn test_author_vec_serialize() {
        let input = vec![
            AuthorProxy {
                username: "user1".to_string(),
                pseudonym: None,
            },
            AuthorProxy {
                username: "user2".to_string(),
                pseudonym: Some("ps".to_string()),
            },
        ];
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), r#"[{"u":"user1"},{"u":"user2","p":"ps"}]"#);
    }

    #[test]
    fn test_author_vec_deserialize() {
        let expected = vec![
            AuthorProxy {
                username: "user1".to_string(),
                pseudonym: None,
            },
            AuthorProxy {
                username: "user2".to_string(),
                pseudonym: Some("ps".to_string()),
            },
        ];
        let obj = from_json::<Vec<AuthorProxy>>(r#"[{"u":"user1"},{"u":"user2","p":"ps"}]"#).unwrap();
        assert_eq!(obj, expected);
    }

    #[test]
    fn test_fandom_vec_serialize() {
        let input = vec![
            FandomProxy("Harry Potter".to_string()),
            FandomProxy("Marvel".to_string()),
        ];
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), r#"["Harry Potter","Marvel"]"#);
    }

    #[test]
    fn test_fandom_vec_deserialize() {
        let expected = vec![
            FandomProxy("Harry Potter".to_string()),
            FandomProxy("Marvel".to_string()),
        ];
        let obj = from_json::<Vec<FandomProxy>>(r#"["Harry Potter","Marvel"]"#).unwrap();
        assert_eq!(obj, expected);
    }

    #[test]
    fn test_series_position_vec_serialize() {
        let input = vec![
            SeriesPositionProxy {
                id: 1,
                name: "S1".to_string(),
                position: 3,
            },
            SeriesPositionProxy {
                id: 2,
                name: "S2".to_string(),
                position: 7,
            },
        ];
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), r#"[{"id":1,"name":"S1","pos":3},{"id":2,"name":"S2","pos":7}]"#);
    }

    #[test]
    fn test_series_position_vec_deserialize() {
        let expected = vec![
            SeriesPositionProxy {
                id: 1,
                name: "S1".to_string(),
                position: 3,
            },
            SeriesPositionProxy {
                id: 2,
                name: "S2".to_string(),
                position: 7,
            },
        ];
        let obj =
            from_json::<Vec<SeriesPositionProxy>>(r#"[{"id":1,"name":"S1","pos":3},{"id":2,"name":"S2","pos":7}]"#)
                .unwrap();
        assert_eq!(obj, expected);
    }

    #[test]
    fn test_tag_vec_serialize() {
        let input = vec![
            TagProxy {
                name: "Draco/Harry".to_string(),
                kind: TagKindProxy::R,
            },
            TagProxy {
                name: "Fluff".to_string(),
                kind: TagKindProxy::F,
            },
        ];
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), r#"[{"n":"Draco/Harry","k":"R"},{"n":"Fluff","k":"F"}]"#);
    }

    #[test]
    fn test_tag_vec_deserialize() {
        let expected = vec![
            TagProxy {
                name: "Draco/Harry".to_string(),
                kind: TagKindProxy::R,
            },
            TagProxy {
                name: "Fluff".to_string(),
                kind: TagKindProxy::F,
            },
        ];
        let obj = from_json::<Vec<TagProxy>>(r#"[{"n":"Draco/Harry","k":"R"},{"n":"Fluff","k":"F"}]"#).unwrap();
        assert_eq!(obj, expected);
    }

    #[test]
    fn test_warning_vec_serialize() {
        let input = vec![WarningProxy::GraphicViolence, WarningProxy::MajorCharacterDeath];
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), r#"["GraphicViolence","MajorCharacterDeath"]"#);
    }

    #[test]
    fn test_warning_vec_deserialize() {
        let expected = vec![WarningProxy::GraphicViolence, WarningProxy::MajorCharacterDeath];
        let obj = from_json::<Vec<WarningProxy>>(r#"["GraphicViolence","MajorCharacterDeath"]"#).unwrap();
        assert_eq!(obj, expected);
    }
}
