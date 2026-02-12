use std::fmt::{Display, Formatter, Result as FmtResult};
use std::{convert::Infallible, str::FromStr};

/// An AO3 user who authored a work.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    type Err = Infallible;
    fn from_str(username: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(username, None::<String>))
    }
}
impl From<String> for Author {
    fn from(username: String) -> Self {
        Self::new(username, None::<String>)
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
