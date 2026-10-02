use super::key::{BatchKey, KeyInputs};
use crate::script::SCRIPT_RECIPE;
use rawr_tts::{
    Alignment, Audio, AudioFormat, CharacterTiming, Continuity, Cost, CostUnit, Fingerprint, Receipt, Synthesis,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub(super) const METADATA_VERSION: u32 = 1;

/// One stored text-to-speech response: its metadata and its audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    pub meta: RecordingMeta,
    pub audio: Audio,
}

/// Whether a recording can be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingStatus {
    Ok,
    /// The provider's alignment did not match the sent text. The audio was
    /// paid for, so it is kept, but it is never used and never paid for again
    /// without an explicit request.
    AlignmentMismatch,
}

/// Everything about a recording except its audio bytes (`recording.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingMeta {
    pub version: u32,
    pub key: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub status: RecordingStatus,
    pub provider: String,
    pub fingerprint: String,
    pub language: Option<String>,
    pub seed: u32,
    pub script_recipe: u32,
    /// The prepared text, exactly as sent.
    pub text: String,
    pub audio: AudioMeta,
    pub alignment: Option<AlignmentMeta>,
    pub receipt: ReceiptMeta,
    pub continuity: ContinuityMeta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioMeta {
    pub file: String,
    pub format: AudioFormatMeta,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormatMeta {
    OggOpus,
    Pcm,
}

/// Alignment in compact form: one character per entry of `characters`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlignmentMeta {
    pub characters: String,
    pub start_ms: Vec<u32>,
    pub end_ms: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptMeta {
    pub request_id: Option<String>,
    pub cost: Option<CostMeta>,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostMeta {
    pub amount: u64,
    pub unit: CostUnitMeta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostUnitMeta {
    Credits,
    Characters,
    Free,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuityMeta {
    pub previous_request_ids: Vec<String>,
    pub next_text: Option<String>,
}

impl Recording {
    /// A recording of a synthesis, made now.
    pub fn new(
        inputs: &KeyInputs<'_>,
        synthesis: Synthesis,
        continuity: &Continuity,
        status: RecordingStatus,
        created_at: OffsetDateTime,
    ) -> Self {
        let (format, sample_rate) = match synthesis.audio.format {
            AudioFormat::OggOpus { sample_rate } => (AudioFormatMeta::OggOpus, sample_rate),
            AudioFormat::Pcm { sample_rate } => (AudioFormatMeta::Pcm, sample_rate),
        };
        let meta = RecordingMeta {
            version: METADATA_VERSION,
            key: BatchKey::new(inputs).to_string(),
            created_at,
            status,
            provider: inputs.provider.to_string(),
            fingerprint: inputs.fingerprint.to_string(),
            language: inputs.language.map(String::from),
            seed: inputs.seed,
            script_recipe: SCRIPT_RECIPE,
            text: inputs.text.to_string(),
            audio: AudioMeta {
                file: format.file_name().to_string(),
                format,
                sample_rate,
            },
            alignment: synthesis.alignment.as_ref().map(AlignmentMeta::from),
            receipt: ReceiptMeta::from(&synthesis.receipt),
            continuity: ContinuityMeta {
                previous_request_ids: continuity.previous_request_ids.clone(),
                next_text: continuity.next_text.clone(),
            },
        };
        Self { meta, audio: synthesis.audio }
    }
}

impl RecordingMeta {
    /// The audio format, rebuilt from the metadata.
    pub fn audio_format(&self) -> AudioFormat {
        match self.audio.format {
            AudioFormatMeta::OggOpus => AudioFormat::OggOpus { sample_rate: self.audio.sample_rate },
            AudioFormatMeta::Pcm => AudioFormat::Pcm { sample_rate: self.audio.sample_rate },
        }
    }

    /// The alignment, rebuilt from the metadata.
    pub fn alignment(&self) -> Option<Alignment> {
        self.alignment.as_ref().map(|alignment| Alignment {
            characters: alignment
                .characters
                .chars()
                .zip(alignment.start_ms.iter().zip(&alignment.end_ms))
                .map(|(character, (&start_ms, &end_ms))| CharacterTiming { character, start_ms, end_ms })
                .collect(),
        })
    }

    /// The receipt's cost, rebuilt from the metadata.
    pub fn cost(&self) -> Option<Cost> {
        self.receipt.cost.map(|cost| Cost {
            amount: cost.amount,
            unit: match cost.unit {
                CostUnitMeta::Credits => CostUnit::Credits,
                CostUnitMeta::Characters => CostUnit::Characters,
                CostUnitMeta::Free => CostUnit::Free,
            },
        })
    }

    /// Whether this recording was made with these key inputs.
    pub fn matches(&self, inputs: &KeyInputs<'_>) -> bool {
        self.key == BatchKey::new(inputs).as_str()
    }

    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::new(self.fingerprint.split(';').filter_map(|entry| entry.split_once('=')))
    }
}

impl AudioFormatMeta {
    pub fn file_name(self) -> &'static str {
        match self {
            Self::OggOpus => "audio.opus",
            Self::Pcm => "audio.pcm",
        }
    }
}

impl From<&Alignment> for AlignmentMeta {
    fn from(alignment: &Alignment) -> Self {
        Self {
            characters: alignment.text(),
            start_ms: alignment.characters.iter().map(|timing| timing.start_ms).collect(),
            end_ms: alignment.characters.iter().map(|timing| timing.end_ms).collect(),
        }
    }
}

impl From<&Receipt> for ReceiptMeta {
    fn from(receipt: &Receipt) -> Self {
        Self {
            request_id: receipt.request_id.clone(),
            cost: receipt.cost.map(|cost| CostMeta {
                amount: cost.amount,
                unit: match cost.unit {
                    CostUnit::Credits => CostUnitMeta::Credits,
                    CostUnit::Characters => CostUnitMeta::Characters,
                    CostUnit::Free => CostUnitMeta::Free,
                },
            }),
            headers: receipt.headers.clone(),
        }
    }
}
