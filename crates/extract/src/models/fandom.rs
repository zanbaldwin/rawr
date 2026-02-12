use std::{convert::Infallible, str::FromStr};

/// A fandom tag associated with a work.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
