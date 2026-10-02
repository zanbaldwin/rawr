//! Audio: PCM, signal processing, and (with a codec feature) encoding and decoding.

mod codec;
mod dsp;
#[cfg(feature = "libopus")]
mod libopus;
#[cfg(feature = "_opus")]
mod mp4;
#[cfg(feature = "_opus")]
mod ogg;

pub use self::codec::{Codec, EncodedAudio};
pub use self::dsp::{TrailingSound, fade};
#[cfg(feature = "libopus")]
pub use self::libopus::LibOpus;

/// The one sample rate the pipeline works at.
pub const SAMPLE_RATE: u32 = 48_000;

/// Mono, signed 16-bit PCM.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pcm {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
}

impl Pcm {
    pub fn new(samples: Vec<i16>, sample_rate: u32) -> Self {
        Self { samples, sample_rate }
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Samples in a number of milliseconds at this sample rate.
    pub fn samples_in(&self, milliseconds: u32) -> usize {
        (u64::from(milliseconds) * u64::from(self.sample_rate) / 1_000) as usize
    }
}
