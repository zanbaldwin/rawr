use rawr_source::Rating;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::time::Duration;

/// What a provider supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub alignment: AlignmentKind,
    /// Batch size the planner should use, in characters.
    pub max_batch_chars: usize,
    /// Default number of requests at the same time.
    pub concurrency: usize,
    /// Limits for request stitching, if the provider supports it.
    pub continuity: Option<ContinuityLimits>,
    pub cost_unit: CostUnit,
    /// Document ratings that need explicit approval before their text is sent.
    pub needs_approval: &'static [Rating],
}

/// How detailed a provider's timing information is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignmentKind {
    Character,
    Word,
    /// No timing: the planner sends one request per sync unit instead.
    None,
}

/// Limits for [`Continuity`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContinuityLimits {
    pub max_previous: usize,
    /// Older previous requests are not accepted.
    pub max_age: Duration,
    pub max_next_chars: usize,
}

/// The unit a provider bills in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CostUnit {
    Credits,
    Characters,
    Free,
}

/// An amount in a provider's [`CostUnit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cost {
    pub amount: u64,
    pub unit: CostUnit,
}

/// Hints that link a request to its neighbours, for a smooth join.
///
/// The caller keeps them inside the provider's [`ContinuityLimits`].
/// Providers ignore what they do not support.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Continuity {
    pub previous_request_ids: Vec<String>,
    pub next_text: Option<String>,
}

/// One text-to-speech request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthesisRequest {
    /// Prepared text (see [`Synthesiser::prepare`](crate::Synthesiser::prepare)).
    pub text: String,
    /// ISO 639 / BCP 47 language code.
    pub language: Option<String>,
    /// The voice role (for example a character's name). `None` is the
    /// language's voice; providers use it too for roles they do not know.
    pub role: Option<String>,
    pub seed: Option<u32>,
    pub continuity: Continuity,
}

/// The result of one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synthesis {
    pub audio: Audio,
    pub alignment: Option<Alignment>,
    pub receipt: Receipt,
}

/// Encoded audio as the provider returned it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audio {
    pub bytes: Vec<u8>,
    pub format: AudioFormat,
}

/// Audio formats providers return. All are mono.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioFormat {
    /// Opus in an Ogg container.
    OggOpus { sample_rate: u32 },
    /// Signed 16-bit little-endian samples, no header.
    Pcm { sample_rate: u32 },
}

/// Timing of every sent character, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alignment {
    pub characters: Vec<CharacterTiming>,
}

/// When one character is spoken, in whole milliseconds from the start of the audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterTiming {
    pub character: char,
    pub start_ms: u32,
    pub end_ms: u32,
}

/// What a request returned besides audio: the record of what was paid for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Receipt {
    pub request_id: Option<String>,
    pub cost: Option<Cost>,
    /// Every response header except credentials and cookies.
    pub headers: Vec<(String, String)>,
}

/// Provider-specific settings that change the audio, in a stable form.
///
/// Keys are sorted, so the same settings always give the same fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fingerprint(String);

impl Alignment {
    /// The aligned characters, as a string.
    pub fn text(&self) -> String {
        self.characters.iter().map(|timing| timing.character).collect()
    }
}

impl Fingerprint {
    pub fn new<K: AsRef<str>, V: AsRef<str>>(entries: impl IntoIterator<Item = (K, V)>) -> Self {
        let mut entries: Vec<(String, String)> =
            entries.into_iter().map(|(key, value)| (key.as_ref().to_string(), value.as_ref().to_string())).collect();
        entries.sort();
        Self(entries.iter().map(|(key, value)| format!("{key}={value}")).collect::<Vec<_>>().join(";"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Fingerprint {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(&self.0)
    }
}
