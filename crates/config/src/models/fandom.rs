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
