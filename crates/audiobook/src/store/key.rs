use crate::script::SCRIPT_RECIPE;
use rawr_tts::Fingerprint;
use std::fmt::{Display, Formatter, Result as FmtResult};

/// Version of the batch key recipe. Bump it whenever [`BatchKey::new`] changes.
pub const KEY_RECIPE: u32 = 1;

/// Everything that changes a batch's audio.
///
/// Continuity (previous request IDs, next text) is deliberately not here:
/// it changes prosody only slightly, and it would tie a recording to its neighbours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyInputs<'a> {
    /// [`Synthesiser::name`](rawr_tts::Synthesiser::name).
    pub provider: &'a str,
    pub fingerprint: &'a Fingerprint,
    pub language: Option<&'a str>,
    /// [`Script::seed`](crate::script::Script::seed).
    pub seed: u32,
    /// The prepared text, exactly as sent.
    pub text: &'a str,
}

/// The address of one recording: a BLAKE3 hash of its [`KeyInputs`], hex-encoded.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BatchKey(String);

impl BatchKey {
    pub fn new(inputs: &KeyInputs<'_>) -> Self {
        let canonical = format!(
            "rawr-batch-key {KEY_RECIPE}\nscript-recipe {SCRIPT_RECIPE}\nprovider {}\nfingerprint {}\nlanguage {}\nseed {}\ntext {}",
            inputs.provider,
            inputs.fingerprint,
            inputs.language.unwrap_or("-"),
            inputs.seed,
            inputs.text,
        );
        Self(blake3::hash(canonical.as_bytes()).to_hex().to_string())
    }

    /// Accept a stored key: 64 lowercase hex characters.
    pub fn parse(value: &str) -> Option<Self> {
        (value.len() == 64 && value.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')))
            .then(|| Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for BatchKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(&self.0)
    }
}
