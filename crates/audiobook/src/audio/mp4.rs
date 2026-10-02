//! A minimal MP4 writer for one mono Opus track.
//!
//! The file always has the same shape, so every size is known before writing:
//! `ftyp`, then `moov` (faststart), then `mdat`. The boxes follow what ffmpeg
//! writes for `-c:a libopus -f mp4 -movflags +faststart`: an `Opus` sample
//! entry with `dOps`, an edit list that removes the encoder delay and the end
//! padding, and an 80 ms `roll` pre-roll group (ISO/IEC 14496-12, Opus in
//! ISOBMFF).

use crate::error::{ErrorKind, Result};

/// Opus always runs at 48 kHz in MP4.
const TIMESCALE: u32 = 48_000;
/// 20 ms frames.
pub(super) const FRAME: u32 = 960;
/// Packets to decode before a seek point: 80 ms (4 × 20 ms).
const ROLL_PACKETS: u32 = 4;

/// An encoded Opus stream.
pub(super) struct OpusTrack<'a> {
    pub packets: &'a [Vec<u8>],
    /// Encoder delay, in 48 kHz samples.
    pub pre_skip: u16,
    /// Real length, in 48 kHz samples.
    pub samples: u64,
}

pub(super) fn write(track: &OpusTrack<'_>) -> Result<Vec<u8>> {
    let count = u32::try_from(track.packets.len()).map_err(|_| too_long())?;
    let media = u32::try_from(track.samples + u64::from(track.pre_skip)).map_err(|_| too_long())?;
    let real = u32::try_from(track.samples).map_err(|_| too_long())?;
    if media > count * FRAME || media <= count.saturating_sub(1) * FRAME {
        exn::bail!(ErrorKind::Codec);
    }
    let data_length: usize = track.packets.iter().map(Vec::len).sum();
    let ftyp = ftyp();
    let moov_length = moov(track, count, media, real, 0).len();
    let data_offset = u32::try_from(ftyp.len() + moov_length + 8).map_err(|_| too_long())?;
    let mdat_size = u32::try_from(8 + data_length).map_err(|_| too_long())?;
    let mut file = Vec::with_capacity(data_offset as usize + data_length);
    file.extend(ftyp);
    file.extend(moov(track, count, media, real, data_offset));
    file.extend(mdat_size.to_be_bytes());
    file.extend(b"mdat");
    for packet in track.packets {
        file.extend(packet);
    }
    Ok(file)
}

fn too_long() -> crate::error::Error {
    exn::Exn::new(ErrorKind::UnsupportedAudio("audio too long for a 32-bit MP4".to_string()))
}

fn plain(kind: &[u8; 4], payload: impl IntoIterator<Item = u8>) -> Vec<u8> {
    let payload: Vec<u8> = payload.into_iter().collect();
    let mut bytes = Vec::with_capacity(8 + payload.len());
    bytes.extend((8 + payload.len() as u32).to_be_bytes());
    bytes.extend(kind);
    bytes.extend(payload);
    bytes
}

fn full(kind: &[u8; 4], version: u8, flags: u32, payload: impl IntoIterator<Item = u8>) -> Vec<u8> {
    let mut header = vec![version];
    header.extend(&flags.to_be_bytes()[1..]);
    plain(kind, header.into_iter().chain(payload))
}

fn be32(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_be_bytes()).collect()
}

const MATRIX: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];

fn ftyp() -> Vec<u8> {
    plain(b"ftyp", [b"isom".as_slice(), &512u32.to_be_bytes(), b"isom", b"iso2", b"mp41"].concat())
}

fn moov(track: &OpusTrack<'_>, count: u32, media: u32, real: u32, data_offset: u32) -> Vec<u8> {
    let mvhd = full(
        b"mvhd",
        0,
        0,
        [
            be32(&[0, 0, TIMESCALE, real, 0x0001_0000]),
            vec![0x01, 0x00, 0, 0],
            be32(&[0, 0]),
            be32(&MATRIX),
            be32(&[0; 6]),
            be32(&[2]),
        ]
        .concat(),
    );
    let tkhd = full(
        b"tkhd",
        0,
        0x3,
        [
            be32(&[0, 0, 1, 0, real, 0, 0]),
            vec![0, 0, 0, 1, 0x01, 0x00, 0, 0],
            be32(&MATRIX),
            be32(&[0, 0]),
        ]
        .concat(),
    );
    let elst = full(b"elst", 0, 0, [be32(&[1, real, u32::from(track.pre_skip)]), vec![0, 1, 0, 0]].concat());
    let mdhd = full(b"mdhd", 0, 0, [be32(&[0, 0, TIMESCALE, media]), vec![0x55, 0xC4, 0, 0]].concat());
    let hdlr = full(
        b"hdlr",
        0,
        0,
        [
            be32(&[0]),
            b"soun".to_vec(),
            be32(&[0, 0, 0]),
            b"SoundHandler\0".to_vec(),
        ]
        .concat(),
    );
    let smhd = full(b"smhd", 0, 0, [0, 0, 0, 0]);
    let dinf = plain(b"dinf", full(b"dref", 0, 0, [be32(&[1]), full(b"url ", 0, 1, [])].concat()));
    let stbl = plain(
        b"stbl",
        [
            stsd(track.pre_skip),
            stts(count, media),
            stsc(count),
            stsz(track.packets),
            stco(data_offset),
            roll(count),
        ]
        .concat(),
    );
    let minf = plain(b"minf", [smhd, dinf, stbl].concat());
    let mdia = plain(b"mdia", [mdhd, hdlr, minf].concat());
    let trak = plain(b"trak", [tkhd, plain(b"edts", elst), mdia].concat());
    plain(b"moov", [mvhd, trak].concat())
}

fn stsd(pre_skip: u16) -> Vec<u8> {
    let dops = plain(
        b"dOps",
        [
            vec![0, 1],
            pre_skip.to_be_bytes().to_vec(),
            TIMESCALE.to_be_bytes().to_vec(),
            vec![0, 0, 0],
        ]
        .concat(),
    );
    let entry = plain(
        b"Opus",
        [
            vec![0; 6],
            vec![0, 1],
            be32(&[0, 0]),
            vec![0, 1, 0, 16, 0, 0, 0, 0],
            be32(&[TIMESCALE << 16]),
            dops,
        ]
        .concat(),
    );
    full(b"stsd", 0, 0, [be32(&[1]), entry].concat())
}

/// Every packet lasts one frame, except the last, which ends the media at pre-skip + length.
fn stts(count: u32, media: u32) -> Vec<u8> {
    let last = media - (count - 1) * FRAME;
    let entries = if last == FRAME {
        be32(&[1, count, FRAME])
    } else if count == 1 {
        be32(&[1, 1, last])
    } else {
        be32(&[2, count - 1, FRAME, 1, last])
    };
    full(b"stts", 0, 0, entries)
}

fn stsc(count: u32) -> Vec<u8> {
    full(b"stsc", 0, 0, be32(&[1, 1, count, 1]))
}

fn stsz(packets: &[Vec<u8>]) -> Vec<u8> {
    let sizes: Vec<u32> = packets.iter().map(|packet| packet.len() as u32).collect();
    full(b"stsz", 0, 0, [be32(&[0, sizes.len() as u32]), be32(&sizes)].concat())
}

fn stco(data_offset: u32) -> Vec<u8> {
    full(b"stco", 0, 0, be32(&[1, data_offset]))
}

/// `sgpd` + `sbgp` `roll`: after the first four packets, every packet needs 80 ms of pre-roll.
fn roll(count: u32) -> Vec<u8> {
    let sgpd = full(b"sgpd", 1, 0, [b"roll".to_vec(), be32(&[2, 1]), (-4i16).to_be_bytes().to_vec()].concat());
    let groups =
        if count > ROLL_PACKETS { be32(&[2, ROLL_PACKETS, 0, count - ROLL_PACKETS, 1]) } else { be32(&[1, count, 0]) };
    let sbgp = full(b"sbgp", 0, 0, [b"roll".to_vec(), groups].concat());
    [sgpd, sbgp].concat()
}
