use std::fmt::{Display, Formatter, Result as FmtResult};
use std::{convert::Infallible, str::FromStr};

/// A fandom tag associated with a work.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Fandom {
    /// Fandom name as it appears on AO3
    pub name: String,
}
impl FromStr for Fandom {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self { name: s.to_string() })
    }
}
impl From<String> for Fandom {
    fn from(name: String) -> Self {
        Self { name }
    }
}
impl From<Fandom> for String {
    fn from(value: Fandom) -> Self {
        value.name
    }
}
impl AsRef<str> for Fandom {
    fn as_ref(&self) -> &str {
        &self.name
    }
}
impl Display for Fandom {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde_json::{from_str as from_json, to_string as to_json};

    #[rstest]
    #[case(Fandom{name: "Harry Potter".to_string()}, r#""Harry Potter""#)]
    fn test_fandom_serialize(#[case] input: Fandom, #[case] expected: impl AsRef<str>) {
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), expected.as_ref());
    }

    #[rstest]
    #[case(Fandom{name: "Harry Potter".to_string()}, r#""Harry Potter""#)]
    fn test_fandom_deserialize(#[case] expected: Fandom, #[case] input: impl AsRef<str>) {
        let obj = from_json::<Fandom>(input.as_ref()).unwrap();
        assert_eq!(obj, expected);
    }

    #[test]
    fn test_fandom_vec_serialize() {
        let input = vec![
            Fandom { name: "Harry Potter".to_string() },
            Fandom { name: "Marvel".to_string() },
        ];
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), r#"["Harry Potter","Marvel"]"#);
    }

    #[test]
    fn test_fandom_vec_deserialize() {
        let expected = vec![
            Fandom { name: "Harry Potter".to_string() },
            Fandom { name: "Marvel".to_string() },
        ];
        let obj = from_json::<Vec<Fandom>>(r#"["Harry Potter","Marvel"]"#).unwrap();
        assert_eq!(obj, expected);
    }
}
