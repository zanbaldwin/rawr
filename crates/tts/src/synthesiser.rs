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

    /// Send one request. The text must already be [`prepare`](Self::prepare)d.
    async fn synthesise(&self, request: &SynthesisRequest) -> Result<Synthesis>;
}

/// A shared, dynamically-dispatched synthesiser.
pub type SynthesiserHandle = Arc<dyn Synthesiser>;
