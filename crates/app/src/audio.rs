//! Config-to-narration wiring: the synthesiser, the recordings store, the
//! audiobook codec, and the document source.

use crate::error::{ErrorKind, Result};
use exn::{OptionExt, ResultExt};
use rawr_audiobook::audio::LibOpus;
use rawr_audiobook::store::Store;
use rawr_config::Config;
use rawr_config::models::{AudioConfig, ELEVENLABS_API_KEY_ENV, ElevenLabsConfig};
use rawr_source::{Ao3Html, Document, Source};
use rawr_storage::BackendHandle;
use rawr_storage::backend::{LocalBackend, ReadOnlyBackend};
use rawr_tts::SynthesiserHandle;
use rawr_tts::elevenlabs::{ApiKey, ElevenLabs, ElevenLabsSettings};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::sync::Arc;

/// What the synthesiser is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Paid requests: the API key is required.
    Narrate,
    /// Planning and export only: no request is sent, so no key is needed.
    Export,
}

/// Where the API key came from. Never the key itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySource {
    /// The config file, or a `RAWR__` environment variable.
    Config,
    /// A `file://` value in the config.
    File(String),
    /// The provider's own environment variable.
    Environment(&'static str),
    /// No key (export only).
    None,
}

impl Display for KeySource {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Config => f.write_str("config"),
            Self::File(path) => write!(f, "file://{path}"),
            Self::Environment(name) => f.write_str(name),
            Self::None => f.write_str("not set"),
        }
    }
}

/// `[audio]`, which every audio command needs.
///
/// # Errors
///
/// [`AudioNotConfigured`](ErrorKind::AudioNotConfigured) if the config has no `[audio]` table.
pub fn audio_config(config: &Config) -> Result<&AudioConfig> {
    config.audio.as_ref().ok_or_raise(|| ErrorKind::AudioNotConfigured)
}

/// Build the configured synthesiser.
///
/// The API key comes from the provider table (the config file or `RAWR__`
/// variables), then from the provider's own environment variable.
///
/// # Errors
///
/// - [`AudioNotConfigured`](ErrorKind::AudioNotConfigured) without `[audio]`
/// - [`MissingApiKey`](ErrorKind::MissingApiKey) for [`Need::Narrate`] without a key
/// - [`Synthesiser`](ErrorKind::Synthesiser) if the provider cannot be set up
pub fn synthesiser(config: &Config, need: Need) -> Result<(SynthesiserHandle, KeySource)> {
    let audio = audio_config(config)?;
    let provider = audio.provider_name().ok_or_raise(|| ErrorKind::AudioNotConfigured)?;
    match provider {
        "elevenlabs" => {
            let table = audio.providers.elevenlabs.as_ref().ok_or_raise(|| ErrorKind::AudioNotConfigured)?;
            elevenlabs(table, need)
        },
        other => exn::bail!(ErrorKind::UnknownProvider(other.to_string())),
    }
}

fn elevenlabs(table: &ElevenLabsConfig, need: Need) -> Result<(SynthesiserHandle, KeySource)> {
    let (key, source) = match &table.api_key {
        Some(key) if !key.is_empty() => {
            let source = key.file().map_or(KeySource::Config, |path| KeySource::File(path.to_string()));
            (key.to_string(), source)
        },
        _ => match std::env::var(ELEVENLABS_API_KEY_ENV) {
            Ok(key) if !key.is_empty() => (key, KeySource::Environment(ELEVENLABS_API_KEY_ENV)),
            _ => (String::new(), KeySource::None),
        },
    };
    if need == Need::Narrate && source == KeySource::None {
        exn::bail!(ErrorKind::MissingApiKey("elevenlabs"));
    }
    let mut settings = ElevenLabsSettings::new(ApiKey::new(key), table.voices.clone().into_iter().collect());
    if let Some(model) = &table.model {
        settings.model = model.clone();
    }
    if let Some(stability) = table.stability {
        settings.stability = stability;
    }
    if let Some(similarity_boost) = table.similarity_boost {
        settings.similarity_boost = similarity_boost;
    }
    settings.concurrency = table.concurrency;
    let provider = ElevenLabs::new(settings).or_raise(|| ErrorKind::Synthesiser)?;
    Ok((Arc::new(provider), source))
}

/// The recordings store at `audio.recordings`.
///
/// When `read_only` is set, every write silently does nothing. Never narrate
/// into a read-only store: paid recordings would be lost.
///
/// # Errors
///
/// - [`AudioNotConfigured`](ErrorKind::AudioNotConfigured) without `[audio]`
/// - [`Recordings`](ErrorKind::Recordings) if the directory cannot be found or opened
pub fn recordings(config: &Config, read_only: bool) -> Result<Store> {
    let directory = audio_config(config)?.recordings_dir().ok_or_raise(|| ErrorKind::Recordings(String::new()))?;
    let display = directory.display().to_string();
    let mut backend: BackendHandle = Arc::new(
        LocalBackend::new("recordings", directory.to_string_lossy(), true)
            .or_raise(|| ErrorKind::Recordings(display))?,
    );
    if read_only {
        backend = Arc::new(ReadOnlyBackend::new(backend));
    }
    Ok(Store::new(backend))
}

/// The audiobook codec, at `audio.bitrate_kbps`.
///
/// # Errors
///
/// [`AudioNotConfigured`](ErrorKind::AudioNotConfigured) without `[audio]`.
pub fn codec(config: &Config) -> Result<LibOpus> {
    Ok(LibOpus::new(audio_config(config)?.bitrate_kbps))
}

/// Turn a decompressed AO3 HTML download into a document.
///
/// CPU-bound: async callers run it in `spawn_blocking`.
///
/// # Errors
///
/// [`Document`](ErrorKind::Document) if the HTML is not a usable AO3 download.
pub fn document(html: &[u8]) -> Result<Document> {
    Ao3Html.to_document(html).or_raise(|| ErrorKind::Document)
}
