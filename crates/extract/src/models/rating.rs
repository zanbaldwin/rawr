use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    str::FromStr,
};

use crate::error::{Error, ErrorKind};

/// Content rating enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rating {
    /// (G) General Audiences
    GeneralAudiences,
    /// (T) Teen And Up Audiences
    TeenAndUp,
    /// (M) Mature
    Mature,
    /// (E) Explicit
    Explicit,
    /// Work is not rated
    NotRated,
}
impl Rating {
    /// Returns the short display string for the rating.
    pub fn as_short_str(&self) -> &'static str {
        match self {
            Rating::GeneralAudiences => "G",
            Rating::TeenAndUp => "T",
            Rating::Mature => "M",
            Rating::Explicit => "E",
            Rating::NotRated => "N",
        }
    }

    /// Returns the full display string for the rating.
    pub fn as_str(&self) -> &'static str {
        match self {
            Rating::GeneralAudiences => "General Audiences",
            Rating::TeenAndUp => "Teen And Up Audiences",
            Rating::Mature => "Mature",
            Rating::Explicit => "Explicit",
            Rating::NotRated => "Not Rated",
        }
    }
}
impl TryFrom<String> for Rating {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().parse()
    }
}
impl FromStr for Rating {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim() {
            "G" | "General Audiences" | "general_audiences" => Self::GeneralAudiences,
            "T" | "Teen And Up Audiences" | "teen_and_up" => Self::TeenAndUp,
            "M" | "Mature" | "mature" => Self::Mature,
            "E" | "Explicit" | "explicit" => Self::Explicit,
            "N" | "Not Rated" | "not_rated" => Self::NotRated,
            _ => exn::bail!(ErrorKind::ParseError {
                field: "rating",
                value: format!("unknown rating: {}", s)
            }),
        })
    }
}

impl Display for Rating {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}
