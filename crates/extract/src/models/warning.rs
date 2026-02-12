use std::fmt::{Display, Formatter, Result as FmtResult};

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
            Warning::NoWarningsApply => "No Archive Warnings Apply",
            Warning::CreatorChoseNotToUse => "Creator Chose Not To Use Archive Warnings",
            Warning::GraphicViolence => "Graphic Depictions Of Violence",
            Warning::MajorCharacterDeath => "Major Character Death",
            Warning::Underage => "Underage",
            Warning::NonCon => "Rape/Non-Con",
        }
    }
}
impl Display for Warning {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}
