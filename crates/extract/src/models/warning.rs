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
        Ok(match s.trim() {
            "No Archive Warnings Apply" | "no_archive_warnings_apply" => Self::NoWarningsApply,
            "Creator Chose Not To Use Archive Warnings"
            | "creator_chose_not_to_use_archive_warnings"
            | "Creator Chose Not To Use"
            | "creator_chose_not_to_use" => Self::CreatorChoseNotToUse,
            "Graphic Depictions Of Violence"
            | "graphic_depictions_of_violence"
            | "Graphic Violence"
            | "graphic_violence" => Self::GraphicViolence,
            "Major Character Death" | "major_character_death" => Self::MajorCharacterDeath,
            "Underage" | "underage" => Self::Underage,
            "Rape/Non-Con" | "rape_non_con" | "rape_noncon" | "Non-Con" | "non_con" | "noncon" => Self::NonCon,
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
