//! Ogg Opus demuxing (RFC 7845).

use crate::error::{ErrorKind, Result};
use exn::ResultExt;
use std::io::Cursor;

/// The packets of a mono Ogg Opus stream, with what is needed to trim the decoded audio.
pub(super) struct OggOpus {
    pub pre_skip: usize,
    pub packets: Vec<Vec<u8>>,
    /// Granule position of the last page: pre-skip plus the real length, in 48 kHz samples.
    pub last_granule: u64,
}

pub(super) fn demux(bytes: &[u8]) -> Result<OggOpus> {
    let mut reader = ogg::reading::PacketReader::new(Cursor::new(bytes));
    let head = reader.read_packet_expected().or_raise(|| ErrorKind::MalformedAudio)?;
    if head.data.len() < 19 || &head.data[..8] != b"OpusHead" {
        exn::bail!(ErrorKind::MalformedAudio);
    }
    if head.data[9] != 1 {
        exn::bail!(ErrorKind::UnsupportedAudio(format!("{} channels", head.data[9])));
    }
    let pre_skip = usize::from(u16::from_le_bytes([head.data[10], head.data[11]]));
    let tags = reader.read_packet_expected().or_raise(|| ErrorKind::MalformedAudio)?;
    if tags.data.len() < 8 || &tags.data[..8] != b"OpusTags" {
        exn::bail!(ErrorKind::MalformedAudio);
    }
    let mut packets = Vec::new();
    let mut last_granule = 0;
    while let Some(packet) = reader.read_packet().or_raise(|| ErrorKind::MalformedAudio)? {
        last_granule = packet.absgp_page();
        packets.push(packet.data);
    }
    Ok(OggOpus { pre_skip, packets, last_granule })
}

/// Drop the pre-skip at the start and the padding after the last granule.
pub(super) fn trim(decoded: Vec<i16>, stream: &OggOpus) -> Vec<i16> {
    let end = usize::try_from(stream.last_granule).unwrap_or(usize::MAX).min(decoded.len());
    let start = stream.pre_skip.min(end);
    decoded[start..end].to_vec()
}
