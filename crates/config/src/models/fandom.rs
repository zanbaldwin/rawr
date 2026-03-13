use serde::Deserialize;
use std::collections::HashMap;

/// Fandom-name normalization and preference rules.
///
/// AO3 has many variant names for the same fandom (e.g.
/// "Spider-Man - All Media Types", "Spider-Man (Marvel) - Fandom"). This
/// config groups them under a single display name and lets users set
/// preferred fandom names for works tagged with multiple fandoms.
#[derive(Debug, Default, Deserialize)]
pub struct FandomConfig {
    /// Maps a canonical display name to the AO3 fandom names it replaces.
    /// During import, any work tagged with an alias is filed under the
    /// canonical name instead.
    #[serde(default)]
    pub renames: HashMap<String, Vec<String>>,
    /// Ordered list of preferred fandom names. When a work belongs to
    /// multiple fandoms, the first matching preference is used as the
    /// primary fandom for path-template rendering. Each entry must be a
    /// key in [`renames`](Self::renames).
    #[serde(default)]
    pub preferences: Vec<String>,
}

// That's a lot of iterating...
impl FandomConfig {
    /// Look up the display name for a fandom.
    /// Returns the display name if the fandom matches a rename rule,
    /// otherwise returns the original name.
    pub fn display_name<'a>(&'a self, fandom: &'a (impl AsRef<str> + ?Sized)) -> &'a str {
        let fandom = fandom.as_ref();
        for (display_name, aliases) in &self.renames {
            if aliases.iter().any(|alias| alias == fandom) {
                return display_name;
            }
        }
        fandom
    }

    /// Select the preferred fandom from a list
    ///
    /// Returns the highest preference that exists in the list of fandoms, or
    /// the first fandom if no preferences match.
    ///
    /// Returns the display name if it has been aliased.
    pub fn preferred_fandom<'a>(&'a self, fandoms: &'a [impl AsRef<str>]) -> Option<&'a str> {
        if fandoms.is_empty() {
            return None;
        }
        // Check preferences in order
        for preferred in &self.preferences {
            // Check if any fandom's display name matches this preference
            if fandoms.iter().any(|f| self.display_name(f) == preferred.as_str()) {
                return Some(preferred.as_str());
            }
            // Check if any fandom directly matches this preference
            if fandoms.iter().any(|f| f.as_ref() == preferred.as_str()) {
                return Some(preferred.as_str());
            }
        }
        // Fall back to first fandom in the list.
        fandoms.first().map(|f| self.display_name(f))
    }
}
