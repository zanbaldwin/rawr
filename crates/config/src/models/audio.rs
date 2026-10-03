use crate::APP_NAME;
use crate::maybe::MaybeFile;
use directories::{BaseDirs, ProjectDirs};
use figment::value::magic::RelativePathBuf;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Providers this build of rawr knows by name.
pub const AUDIO_PROVIDERS: [&str; 1] = ["elevenlabs"];
/// The environment variable ElevenLabs' own tools read the API key from.
/// rawr reads it last, after the config file and `RAWR__` variables.
pub const ELEVENLABS_API_KEY_ENV: &str = "ELEVENLABS_API_KEY";
/// The book's audio bitrate when `audio.bitrate_kbps` is not set.
pub const DEFAULT_BITRATE_KBPS: u32 = 32;

/// Narration and audiobook settings (`[audio]`).
///
/// Everything that changes the recorded audio is set here, not by flags, so
/// that recordings stay findable from one run to the next.
#[derive(Debug, Deserialize)]
pub struct AudioConfig {
    /// The text-to-speech provider. Optional while only one provider table
    /// is configured: see [`provider_name`](Self::provider_name).
    pub provider: Option<String>,
    /// Where paid recordings are kept. Relative paths are resolved against
    /// the config file's directory, and a leading `~/` against the home
    /// directory: see [`recordings_dir`](Self::recordings_dir).
    pub recordings: Option<RelativePathBuf>,
    /// The audiobook's Opus bitrate, in kb/s.
    #[serde(default = "default_bitrate")]
    pub bitrate_kbps: u32,
    /// Stop a narration run before it sends more than this many characters.
    pub max_chars: Option<usize>,
    #[serde(default)]
    pub providers: AudioProviders,
}

/// One table for each provider (`[audio.providers.*]`).
#[derive(Debug, Default, Deserialize)]
pub struct AudioProviders {
    pub elevenlabs: Option<ElevenLabsConfig>,
}

/// `[audio.providers.elevenlabs]`. Unset values use rawr-tts' defaults.
#[derive(Debug, Default, Deserialize)]
pub struct ElevenLabsConfig {
    /// Supports [`MaybeFile`] (`file:///run/secrets/elevenlabs`).
    pub api_key: Option<MaybeFile>,
    pub model: Option<String>,
    pub stability: Option<f32>,
    pub similarity_boost: Option<f32>,
    pub concurrency: Option<usize>,
    /// Language code → voice ID.
    #[serde(default)]
    pub voices: BTreeMap<String, String>,
}

impl AudioConfig {
    /// `provider`, or the only configured provider table when it is not set.
    pub fn provider_name(&self) -> Option<&str> {
        if let Some(provider) = &self.provider {
            return Some(provider);
        }
        let configured: Vec<&str> = self.configured_providers().collect();
        match configured.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }

    /// The providers that have a table.
    pub fn configured_providers(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.providers.elevenlabs.as_ref().map(|_| "elevenlabs").into_iter()
    }

    /// The recordings directory: `recordings`, with a leading `~/` expanded,
    /// or `recordings` in the platform data directory
    /// (`~/.local/share/rawr/recordings` on Linux).
    pub fn recordings_dir(&self) -> Option<PathBuf> {
        match &self.recordings {
            Some(path) => match path.original().strip_prefix("~") {
                Ok(rest) => BaseDirs::new().map(|dirs| dirs.home_dir().join(rest)),
                Err(_) => Some(path.relative()),
            },
            None => ProjectDirs::from("", "", APP_NAME).map(|dirs| dirs.data_dir().join("recordings")),
        }
    }
}

fn default_bitrate() -> u32 {
    DEFAULT_BITRATE_KBPS
}
