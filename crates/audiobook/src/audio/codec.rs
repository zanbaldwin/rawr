use super::Pcm;
use crate::error::Result;
use rawr_tts::Audio;

/// Encoded audio, ready to put in a book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedAudio {
    pub bytes: Vec<u8>,
    /// The media type to declare in the package, for example `audio/mp4`.
    pub media_type: &'static str,
    /// The file extension, without a dot.
    pub extension: &'static str,
    /// Length of the audio in samples, without encoder delay or padding.
    pub samples: u64,
}

/// Turns provider audio into PCM, and PCM into the book's audio format.
///
/// Synchronous and CPU-bound: async callers use `spawn_blocking`.
pub trait Codec: Send + Sync {
    /// Provider audio → 48 kHz mono PCM. Encoder delay and padding are removed.
    ///
    /// # Errors
    ///
    /// [`UnsupportedAudio`](crate::error::ErrorKind::UnsupportedAudio) for audio the
    /// codec cannot handle, [`MalformedAudio`](crate::error::ErrorKind::MalformedAudio)
    /// for audio it cannot read.
    fn decode(&self, audio: &Audio) -> Result<Pcm>;

    /// 48 kHz mono PCM → the book's audio format.
    ///
    /// # Errors
    ///
    /// [`UnsupportedAudio`](crate::error::ErrorKind::UnsupportedAudio) for another sample
    /// rate, [`Codec`](crate::error::ErrorKind::Codec) if the encoder fails.
    fn encode(&self, pcm: &Pcm) -> Result<EncodedAudio>;
}
