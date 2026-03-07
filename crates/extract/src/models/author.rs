use crate::error::{Error, ErrorKind};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::str::FromStr;

/// An AO3 user who authored a work.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Author {
    /// AO3 username
    pub username: String,
    /// Pseudonym (display name)
    pub pseudonym: Option<String>,
}
impl Author {
    pub fn new<P: Into<String>>(username: impl Into<String>, pseudonym: Option<P>) -> Self {
        let username = username.into();
        let pseudonym = pseudonym.map(Into::into).filter(|p: &String| *p != username);
        Self { username, pseudonym }
    }
}
impl FromStr for Author {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = |c| ['(', ')'].contains(&c) || c.is_whitespace();
        let s = s.trim_matches(t);
        if s.is_empty() {
            return Err(ErrorKind::ParseError { field: "author", value: s.to_string() }.into());
        }
        match s.rsplit_once(" (") {
            None => Ok(Self::new(s, None::<&str>)),
            Some((pseudonym, rest)) => {
                let username = rest.trim_matches(t);
                Ok(Self::new(username.trim_matches(t), Some(pseudonym.trim_matches(t))))
            },
        }
    }
}
impl TryFrom<String> for Author {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl<U: Into<String>, P: Into<String>> From<(U, Option<P>)> for Author {
    fn from((username, pseudonym): (U, Option<P>)) -> Self {
        Self::new(username, pseudonym)
    }
}
impl Display for Author {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match &self.pseudonym {
            Some(pseud) => write!(f, "{} ({})", pseud, self.username),
            None => write!(f, "{}", self.username),
        }
    }
}
#[cfg(feature = "serde")]
impl serde::Serialize for Author {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Author {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;
    use serde_json::{from_str as from_json, to_string as to_json};

    #[rstest]
    #[case(Author{username: "user123".to_string(), pseudonym: None}, r#""user123""#)]
    #[case(Author{username: "user321".to_string(), pseudonym: Some("another1".to_string())}, r#""another1 (user321)""#)]
    fn test_author_serialize(#[case] input: Author, #[case] expected: impl AsRef<str>) {
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), expected.as_ref());
    }

    #[rstest]
    #[case(Author{username: "user123".to_string(), pseudonym: None}, r#""user123""#)]
    #[case(Author{username: "user321".to_string(), pseudonym: Some("another1".to_string())}, r#""another1 (user321)""#)]
    fn test_author_deserialize(#[case] expected: Author, #[case] input: impl AsRef<str>) {
        let obj = from_json::<Author>(input.as_ref()).unwrap();
        assert_eq!(obj, expected);
    }

    #[test]
    fn test_author_vec_serialize() {
        let input = vec![
            Author {
                username: "user1".to_string(),
                pseudonym: None,
            },
            Author {
                username: "user2".to_string(),
                pseudonym: Some("ps".to_string()),
            },
        ];
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), r#"["user1","ps (user2)"]"#);
    }

    #[test]
    fn test_author_vec_deserialize() {
        let expected = vec![
            Author {
                username: "user1".to_string(),
                pseudonym: None,
            },
            Author {
                username: "user2".to_string(),
                pseudonym: Some("ps".to_string()),
            },
        ];
        let obj = from_json::<Vec<Author>>(r#"["user1","ps (user2)"]"#).unwrap();
        assert_eq!(obj, expected);
    }
}
