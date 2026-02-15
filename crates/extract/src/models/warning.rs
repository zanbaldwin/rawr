use super::sanitize;
use crate::error::{Error, ErrorKind};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::str::FromStr;

/// Archive warning enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Warning {
    /// No Archive Warnings Apply
    NoWarningsApply,
    /// Creator Chose Not To Use Archive Warnings
    CreatorChoseNotToUse,
    /// Graphic Depictions Of Violence
    GraphicViolence,
    /// Major Character Death
    MajorCharacterDeath,
    /// Underage
    Underage,
    /// Rape/Non-Con
    NonCon,
}
impl Warning {
    /// Returns the display string for the warning.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NoWarningsApply => "No Archive Warnings Apply",
            Self::CreatorChoseNotToUse => "Creator Chose Not To Use Archive Warnings",
            Self::GraphicViolence => "Graphic Depictions Of Violence",
            Self::MajorCharacterDeath => "Major Character Death",
            Self::Underage => "Underage",
            Self::NonCon => "Rape/Non-Con",
        }
    }
}
impl FromStr for Warning {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let sanitized = sanitize(s);
        Ok(match sanitized.as_str() {
            "noarchivewarningsapply" | "nowarningsapply" => Self::NoWarningsApply,
            "creatorchosenottousearchivewarnings"
            | "creatorchosenottousewarnings"
            | "creatorchosenottouse"
            | "chosenottousearchivewarnings"
            | "chosenottousewarnings"
            | "chosenottouse" => Self::CreatorChoseNotToUse,
            "graphicdepictionsofviolence" | "graphicviolence" | "depictionsofviolence" => Self::GraphicViolence,
            "majorcharacterdeath" => Self::MajorCharacterDeath,
            "underage" => Self::Underage,
            "rapenoncon" | "noncon" => Self::NonCon,
            _ => exn::bail!(ErrorKind::ParseError {
                field: "warnings",
                value: format!("unknown warning: {}", s)
            }),
        })
    }
}
impl TryFrom<String> for Warning {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().parse()
    }
}
impl Display for Warning {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}
