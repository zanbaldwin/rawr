use super::key::BatchKey;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Which recordings the latest plan of a document uses.
///
/// Nothing reads a ledger to resume: recordings are found by key. Ledgers
/// only tell which recordings belong to which document, for clearing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub identifier: String,
    pub title: String,
    /// Hash of the source the plan was made from (AO3: `Version::hash`), if known.
    pub source_hash: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub planned_at: OffsetDateTime,
    /// The voice's display name, for the narrator credit, when the provider knows it.
    #[serde(default)]
    pub voice_name: Option<String>,
    pub sections: Vec<LedgerSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerSection {
    pub number: u32,
    pub seed: u32,
    pub keys: Vec<String>,
}

impl Ledger {
    /// Every valid key in the ledger.
    pub fn keys(&self) -> impl Iterator<Item = BatchKey> + '_ {
        self.sections.iter().flat_map(|section| section.keys.iter()).filter_map(|key| BatchKey::parse(key))
    }
}
