use crate::error::Result;
use crate::types::{Capabilities, Fingerprint, Synthesis, SynthesisRequest};
use async_trait::async_trait;
use std::sync::Arc;

/// A text-to-speech provider.
#[async_trait]
pub trait Synthesiser: Send + Sync {
    /// Stable provider name. It is part of every batch key, so never change it.
    fn name(&self) -> &str;

    /// What this provider supports.
    fn capabilities(&self) -> &Capabilities;

    /// Provider-specific cleaning of the text to send.
    ///
    /// Must keep the character count: alignment indices are matched against
    /// offsets in the original text.
    fn prepare(&self, text: &str) -> String;

    /// Everything provider-specific that changes the audio for this language
    /// (model, voice, voice settings, output format), in a stable form.
    ///
    /// # Errors
    ///
    /// [`NoVoice`](crate::error::ErrorKind::NoVoice) if no voice is configured for the language.
    fn fingerprint(&self, language: Option<&str>) -> Result<Fingerprint>;

    /// [`fingerprint`](Self::fingerprint) for a voice role. `None` gives the
    /// same fingerprint as `fingerprint`. The default ignores roles: every
    /// role speaks with the language's voice.
    ///
    /// # Errors
    ///
    /// [`NoVoice`](crate::error::ErrorKind::NoVoice) if no voice is configured for the language.
    fn role_fingerprint(&self, language: Option<&str>, role: Option<&str>) -> Result<Fingerprint> {
        let _ = role;
        self.fingerprint(language)
    }

    /// Send one request. The text must already be [`prepare`](Self::prepare)d.
    async fn synthesise(&self, request: &SynthesisRequest) -> Result<Synthesis>;

    /// The display name of the voice for this language, for the narrator
    /// credit. Never paid for. `None` when the provider cannot tell.
    ///
    /// # Errors
    ///
    /// [`NoVoice`](crate::error::ErrorKind::NoVoice) if no voice is configured
    /// for the language, or the provider's errors for a failed lookup.
    async fn voice_name(&self, language: Option<&str>) -> Result<Option<String>> {
        self.fingerprint(language)?;
        Ok(None)
    }

    /// [`voice_name`](Self::voice_name) for a voice role. The default ignores roles.
    ///
    /// # Errors
    ///
    /// As [`voice_name`](Self::voice_name).
    async fn role_voice_name(&self, language: Option<&str>, role: Option<&str>) -> Result<Option<String>> {
        let _ = role;
        self.voice_name(language).await
    }
}

/// A shared, dynamically-dispatched synthesiser.
pub type SynthesiserHandle = Arc<dyn Synthesiser>;
