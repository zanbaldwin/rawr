//! ElevenLabs text to speech (`with-timestamps` endpoint).

use crate::error::{ErrorKind, Result};
use crate::synthesiser::Synthesiser;
use crate::types::{
    Alignment, AlignmentKind, Audio, AudioFormat, Capabilities, CharacterTiming, ContinuityLimits, Cost, CostUnit,
    Fingerprint, Receipt, Synthesis, SynthesisRequest,
};
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use exn::{OptionExt, ResultExt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::time::Duration;

const BASE_URL: &str = "https://api.elevenlabs.io";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_CONCURRENCY: usize = 3;
const PRIVATE_HEADERS: &[&str] = &["authorization", "cookie", "set-cookie", "xi-api-key"];
const CONTINUITY: ContinuityLimits = ContinuityLimits {
    max_previous: 3,
    max_age: Duration::from_secs(110 * 60),
    max_next_chars: 300,
};

/// An API key. It is never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
}

impl Debug for ApiKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("ApiKey(***)")
    }
}

/// Audio formats requested from ElevenLabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputFormat {
    Opus { sample_rate: u32, bitrate_kbps: u32 },
    Pcm { sample_rate: u32 },
}

impl OutputFormat {
    /// The `output_format` query value, for example `opus_48000_128`.
    pub fn as_query(&self) -> String {
        match self {
            Self::Opus { sample_rate, bitrate_kbps } => format!("opus_{sample_rate}_{bitrate_kbps}"),
            Self::Pcm { sample_rate } => format!("pcm_{sample_rate}"),
        }
    }

    fn audio_format(&self) -> AudioFormat {
        match *self {
            Self::Opus { sample_rate, .. } => AudioFormat::OggOpus { sample_rate },
            Self::Pcm { sample_rate } => AudioFormat::Pcm { sample_rate },
        }
    }
}

/// Settings for [`ElevenLabs`].
#[derive(Debug, Clone, PartialEq)]
pub struct ElevenLabsSettings {
    pub api_key: ApiKey,
    pub model: String,
    pub output_format: OutputFormat,
    pub stability: f32,
    pub similarity_boost: f32,
    /// Language code → voice ID.
    pub voices: BTreeMap<String, String>,
    /// Overrides the default of 3 requests at the same time.
    pub concurrency: Option<usize>,
}

impl ElevenLabsSettings {
    /// Settings with the defaults proven in purr: `eleven_v4`, Opus 48 kHz 128 kb/s,
    /// stability 0.35, similarity boost 0.75.
    pub fn new(api_key: ApiKey, voices: BTreeMap<String, String>) -> Self {
        Self {
            api_key,
            model: "eleven_v4".to_string(),
            output_format: OutputFormat::Opus { sample_rate: 48_000, bitrate_kbps: 128 },
            stability: 0.35,
            similarity_boost: 0.75,
            voices,
            concurrency: None,
        }
    }
}

/// The ElevenLabs provider.
pub struct ElevenLabs {
    settings: ElevenLabsSettings,
    capabilities: Capabilities,
    client: reqwest::Client,
}

impl Debug for ElevenLabs {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("ElevenLabs").field("settings", &self.settings).finish_non_exhaustive()
    }
}

#[derive(Serialize)]
struct RequestBody<'a> {
    text: &'a str,
    model_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    language_code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u32>,
    voice_settings: VoiceSettings,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    previous_request_ids: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    next_text: Option<&'a str>,
}

#[derive(Serialize)]
struct VoiceSettings {
    stability: f32,
    similarity_boost: f32,
}

#[derive(Deserialize)]
struct ResponseBody {
    audio_base64: String,
    alignment: Option<AlignmentBody>,
}

#[derive(Deserialize)]
struct AlignmentBody {
    characters: Vec<String>,
    character_start_times_seconds: Vec<f64>,
    character_end_times_seconds: Vec<f64>,
}

#[derive(Deserialize)]
struct ErrorBody {
    detail: Option<ErrorDetail>,
}

#[derive(Deserialize)]
struct ErrorDetail {
    status: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

impl ElevenLabs {
    /// # Errors
    ///
    /// [`Setup`](ErrorKind::Setup) if the HTTP client cannot be built.
    pub fn new(settings: ElevenLabsSettings) -> Result<Self> {
        let client = reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build().or_raise(|| ErrorKind::Setup)?;
        let capabilities = Capabilities {
            alignment: AlignmentKind::Character,
            max_batch_chars: 2_000,
            concurrency: settings.concurrency.unwrap_or(DEFAULT_CONCURRENCY),
            continuity: Some(CONTINUITY),
            cost_unit: CostUnit::Credits,
        };
        Ok(Self { settings, capabilities, client })
    }

    /// The request URL for a language.
    ///
    /// # Errors
    ///
    /// [`NoVoice`](ErrorKind::NoVoice) if no voice is configured for the language.
    pub fn request_url(&self, language: Option<&str>) -> Result<String> {
        Ok(format!(
            "{BASE_URL}/v1/text-to-speech/{}/with-timestamps?output_format={}",
            self.voice(language)?,
            self.settings.output_format.as_query()
        ))
    }

    /// The JSON request body. Continuity is cut to the provider's limits:
    /// the last three previous request IDs and the first 300 characters of the next text.
    pub fn request_json(&self, request: &SynthesisRequest) -> Vec<u8> {
        let previous = &request.continuity.previous_request_ids;
        let previous = &previous[previous.len().saturating_sub(CONTINUITY.max_previous)..];
        let next_text = request.continuity.next_text.as_deref().map(|text| {
            match text.char_indices().nth(CONTINUITY.max_next_chars) {
                Some((end, _)) => &text[..end],
                None => text,
            }
        });
        let body = RequestBody {
            text: &request.text,
            model_id: &self.settings.model,
            language_code: request.language.as_deref(),
            seed: request.seed,
            voice_settings: VoiceSettings {
                stability: self.settings.stability,
                similarity_boost: self.settings.similarity_boost,
            },
            previous_request_ids: previous,
            next_text,
        };
        serde_json::to_vec(&body).expect("request body always serialises")
    }

    /// Turn an HTTP response into a [`Synthesis`] or an error, without any network.
    ///
    /// `sent_text` is the prepared text that was sent. The alignment must match it.
    ///
    /// # Errors
    ///
    /// The [`ErrorKind`] for the HTTP status, [`MalformedResponse`](ErrorKind::MalformedResponse)
    /// for a body that cannot be decoded, and [`AlignmentMismatch`](ErrorKind::AlignmentMismatch)
    /// (with the paid synthesis) when the alignment does not match `sent_text`.
    pub fn parse_response(
        &self,
        status: u16,
        headers: &[(String, String)],
        body: &[u8],
        sent_text: &str,
    ) -> Result<Synthesis> {
        if !(200..300).contains(&status) {
            exn::bail!(classify(status, body));
        }
        let response: ResponseBody = serde_json::from_slice(body).or_raise(|| ErrorKind::MalformedResponse)?;
        let audio = BASE64.decode(response.audio_base64.as_bytes()).or_raise(|| ErrorKind::MalformedResponse)?;
        let headers: Vec<(String, String)> = headers
            .iter()
            .filter(|(name, _)| !PRIVATE_HEADERS.contains(&name.to_ascii_lowercase().as_str()))
            .cloned()
            .collect();
        let header = |name: &str| {
            headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
        };
        let receipt = Receipt {
            request_id: header("request-id").map(String::from),
            cost: header("character-cost")
                .and_then(|value| value.parse().ok())
                .map(|amount| Cost { amount, unit: CostUnit::Credits }),
            headers: headers.clone(),
        };
        let alignment = response.alignment.map(alignment).transpose()?;
        let matches = alignment.as_ref().is_some_and(|alignment| alignment.text() == sent_text);
        let synthesis = Synthesis {
            audio: Audio {
                bytes: audio,
                format: self.settings.output_format.audio_format(),
            },
            alignment,
            receipt,
        };
        if !matches {
            exn::bail!(ErrorKind::AlignmentMismatch(Box::new(synthesis)));
        }
        Ok(synthesis)
    }

    fn voice(&self, language: Option<&str>) -> Result<&str> {
        language
            .and_then(|language| self.settings.voices.get(language))
            .map(String::as_str)
            .ok_or_raise(|| ErrorKind::NoVoice(language.map(String::from)))
    }
}

#[async_trait]
impl Synthesiser for ElevenLabs {
    fn name(&self) -> &str {
        "elevenlabs"
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
        Ok(Fingerprint::new([
            ("model", self.settings.model.clone()),
            ("voice", self.voice(language)?.to_string()),
            ("stability", self.settings.stability.to_string()),
            ("similarity_boost", self.settings.similarity_boost.to_string()),
            ("output_format", self.settings.output_format.as_query()),
        ]))
    }

    async fn synthesise(&self, request: &SynthesisRequest) -> Result<Synthesis> {
        let url = self.request_url(request.language.as_deref())?;
        let response = self
            .client
            .post(url)
            .header("xi-api-key", &self.settings.api_key.0)
            .header("content-type", "application/json")
            .body(self.request_json(request))
            .send()
            .await
            .or_raise(|| ErrorKind::Unavailable)?;
        let status = response.status().as_u16();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(name, value)| (name.as_str().to_string(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
            .collect();
        let body = response.bytes().await.or_raise(|| ErrorKind::Unavailable)?;
        tracing::trace!(status, body = %String::from_utf8_lossy(&body), "elevenlabs response");
        self.parse_response(status, &headers, &body, &request.text)
    }
}

fn alignment(body: AlignmentBody) -> Result<Alignment> {
    let count = body.characters.len();
    if body.character_start_times_seconds.len() != count || body.character_end_times_seconds.len() != count {
        exn::bail!(ErrorKind::MalformedResponse);
    }
    let mut characters = Vec::with_capacity(count);
    for ((text, start), end) in
        body.characters.iter().zip(body.character_start_times_seconds).zip(body.character_end_times_seconds)
    {
        let mut chars = text.chars();
        let (Some(character), None) = (chars.next(), chars.next()) else {
            exn::bail!(ErrorKind::MalformedResponse);
        };
        characters.push(CharacterTiming {
            character,
            start_ms: milliseconds(start),
            end_ms: milliseconds(end),
        });
    }
    Ok(Alignment { characters })
}

fn milliseconds(seconds: f64) -> u32 {
    (seconds * 1_000.0).round().max(0.0) as u32
}

fn classify(status: u16, body: &[u8]) -> ErrorKind {
    let detail = serde_json::from_slice::<ErrorBody>(body).ok().and_then(|body| body.detail);
    let is = |value: &str| {
        detail
            .as_ref()
            .is_some_and(|detail| detail.status.as_deref() == Some(value) || detail.code.as_deref() == Some(value))
    };
    match status {
        429 => ErrorKind::RateLimited,
        500..=599 => ErrorKind::Unavailable,
        401 if is("quota_exceeded") => ErrorKind::QuotaExceeded,
        401 | 403 => ErrorKind::Unauthorised,
        _ => ErrorKind::Rejected(detail.and_then(|detail| detail.message).unwrap_or_else(|| format!("HTTP {status}"))),
    }
}
