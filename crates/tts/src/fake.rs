use crate::error::{ErrorKind, Result};
use crate::synthesiser::Synthesiser;
use crate::types::{
    Alignment, AlignmentKind, Audio, AudioFormat, Capabilities, CharacterTiming, ContinuityLimits, Cost, CostUnit,
    Fingerprint, Receipt, Synthesis, SynthesisRequest,
};
use async_trait::async_trait;
use rawr_source::Rating;
use std::f32::consts::TAU;
use std::time::Duration;

const SAMPLE_RATE: u32 = 48_000;
/// About English speaking speed (purr measured 945–1,000 characters a minute).
const MS_PER_CHARACTER: u32 = 60;
const TONE_HZ: f32 = 220.0;
const TONE_AMPLITUDE: f32 = 3_000.0;

/// A deterministic provider for tests. No network, no cost.
///
/// Every character lasts the same time: a quiet tone for letters and digits,
/// silence for everything else. The alignment is exact. Its capabilities
/// match ElevenLabs, so the whole pipeline can run against it.
#[derive(Debug, Clone)]
pub struct FakeSynthesiser {
    voice: String,
    languages: Option<Vec<String>>,
    capabilities: Capabilities,
}

impl FakeSynthesiser {
    /// A fake with the given voice name, for every language.
    pub fn new(voice: impl Into<String>) -> Self {
        Self {
            voice: voice.into(),
            languages: None,
            capabilities: Capabilities {
                alignment: AlignmentKind::Character,
                max_batch_chars: 2_000,
                concurrency: 3,
                continuity: Some(ContinuityLimits {
                    max_previous: 3,
                    max_age: Duration::from_secs(110 * 60),
                    max_next_chars: 300,
                }),
                cost_unit: CostUnit::Characters,
                needs_approval: &[Rating::Explicit, Rating::Unrated],
            },
        }
    }

    /// Only these languages have a voice; others give [`NoVoice`](ErrorKind::NoVoice).
    pub fn with_languages<S: Into<String>>(mut self, languages: impl IntoIterator<Item = S>) -> Self {
        self.languages = Some(languages.into_iter().map(Into::into).collect());
        self
    }
}

#[async_trait]
impl Synthesiser for FakeSynthesiser {
    fn name(&self) -> &str {
        "fake"
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn prepare(&self, text: &str) -> String {
        let prepared: String = text
            .chars()
            .map(|character| match character {
                '[' => '(',
                ']' => ')',
                other => other,
            })
            .collect();
        debug_assert_eq!(prepared.chars().count(), text.chars().count());
        prepared
    }

    fn fingerprint(&self, language: Option<&str>) -> Result<Fingerprint> {
        if let Some(languages) = &self.languages
            && !language.is_some_and(|language| languages.iter().any(|known| known == language))
        {
            exn::bail!(ErrorKind::NoVoice(language.map(String::from)));
        }
        Ok(Fingerprint::new([
            ("voice", self.voice.as_str()),
            ("ms_per_character", &MS_PER_CHARACTER.to_string()),
            ("format", &format!("pcm_{SAMPLE_RATE}")),
        ]))
    }

    async fn voice_name(&self, language: Option<&str>) -> Result<Option<String>> {
        self.fingerprint(language)?;
        Ok(Some(self.voice.clone()))
    }

    async fn synthesise(&self, request: &SynthesisRequest) -> Result<Synthesis> {
        self.fingerprint(request.language.as_deref())?;
        let samples_per_character = (SAMPLE_RATE * MS_PER_CHARACTER / 1_000) as usize;
        let mut bytes = Vec::with_capacity(request.text.chars().count() * samples_per_character * 2);
        let mut characters = Vec::new();
        for (index, character) in request.text.chars().enumerate() {
            let start_ms = index as u32 * MS_PER_CHARACTER;
            characters.push(CharacterTiming {
                character,
                start_ms,
                end_ms: start_ms + MS_PER_CHARACTER,
            });
            let voiced = character.is_alphanumeric();
            for sample in 0..samples_per_character {
                let value = if voiced {
                    (TONE_AMPLITUDE * (TAU * TONE_HZ * sample as f32 / SAMPLE_RATE as f32).sin()) as i16
                } else {
                    0
                };
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let length = characters.len() as u64;
        Ok(Synthesis {
            audio: Audio {
                bytes,
                format: AudioFormat::Pcm { sample_rate: SAMPLE_RATE },
            },
            alignment: Some(Alignment { characters }),
            receipt: Receipt {
                request_id: Some(format!("fake-{}-{length}", request.seed.unwrap_or_default())),
                cost: Some(Cost {
                    amount: length,
                    unit: CostUnit::Characters,
                }),
                headers: Vec::new(),
            },
        })
    }
}
