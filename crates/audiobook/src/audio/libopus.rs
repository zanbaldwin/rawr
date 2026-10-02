use super::codec::{Codec, EncodedAudio};
use super::mp4::{self, FRAME, OpusTrack};
use super::ogg;
use super::{Pcm, SAMPLE_RATE};
use crate::error::{ErrorKind, Result};
use exn::ResultExt;
use rawr_tts::{Audio, AudioFormat};

/// Longest Opus frame: 120 ms.
const MAX_FRAME_SAMPLES: usize = 5_760;
const MAX_PACKET_BYTES: usize = 4_000;

/// [`Codec`] backed by libopus (built from source). Writes Opus in MP4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibOpus {
    bitrate_kbps: u32,
}

impl LibOpus {
    pub fn new(bitrate_kbps: u32) -> Self {
        Self { bitrate_kbps }
    }
}

impl Default for LibOpus {
    /// 32 kb/s, the rate that passed the device test.
    fn default() -> Self {
        Self::new(32)
    }
}

impl Codec for LibOpus {
    fn decode(&self, audio: &Audio) -> Result<Pcm> {
        match audio.format {
            AudioFormat::OggOpus { .. } => {
                let stream = ogg::demux(&audio.bytes)?;
                let mut decoder =
                    opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono).or_raise(|| ErrorKind::Codec)?;
                let mut decoded = Vec::new();
                let mut buffer = vec![0i16; MAX_FRAME_SAMPLES];
                for packet in &stream.packets {
                    let length = decoder.decode(packet, &mut buffer, false).or_raise(|| ErrorKind::MalformedAudio)?;
                    decoded.extend_from_slice(&buffer[..length]);
                }
                Ok(Pcm::new(ogg::trim(decoded, &stream), SAMPLE_RATE))
            },
            AudioFormat::Pcm { sample_rate } if sample_rate == SAMPLE_RATE => {
                if !audio.bytes.len().is_multiple_of(2) {
                    exn::bail!(ErrorKind::MalformedAudio);
                }
                let samples = audio.bytes.as_chunks::<2>().0.iter().map(|pair| i16::from_le_bytes(*pair)).collect();
                Ok(Pcm::new(samples, SAMPLE_RATE))
            },
            AudioFormat::Pcm { sample_rate } => {
                exn::bail!(ErrorKind::UnsupportedAudio(format!("PCM at {sample_rate} Hz")))
            },
        }
    }

    fn encode(&self, pcm: &Pcm) -> Result<EncodedAudio> {
        if pcm.sample_rate != SAMPLE_RATE {
            exn::bail!(ErrorKind::UnsupportedAudio(format!("PCM at {} Hz", pcm.sample_rate)));
        }
        let mut encoder = opus::Encoder::new(SAMPLE_RATE, opus::Channels::Mono, opus::Application::Audio)
            .or_raise(|| ErrorKind::Codec)?;
        encoder.set_bitrate(opus::Bitrate::Bits(self.bitrate_kbps as i32 * 1_000)).or_raise(|| ErrorKind::Codec)?;
        let pre_skip = u16::try_from(encoder.get_lookahead().or_raise(|| ErrorKind::Codec)?)
            .map_err(|_| exn::Exn::new(ErrorKind::Codec))?;
        let frame = FRAME as usize;
        let mut input = pcm.samples.clone();
        input.resize((pcm.len() + usize::from(pre_skip)).div_ceil(frame).max(1) * frame, 0);
        let mut packets = Vec::with_capacity(input.len() / frame);
        let mut buffer = vec![0u8; MAX_PACKET_BYTES];
        for chunk in input.chunks_exact(frame) {
            let length = encoder.encode(chunk, &mut buffer).or_raise(|| ErrorKind::Codec)?;
            packets.push(buffer[..length].to_vec());
        }
        let samples = pcm.len() as u64;
        let bytes = mp4::write(&OpusTrack { packets: &packets, pre_skip, samples })?;
        Ok(EncodedAudio {
            bytes,
            media_type: "audio/mp4",
            extension: "mp4",
            samples,
        })
    }
}
