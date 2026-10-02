//! One chapter's audio: a section's recordings, trimmed and joined with
//! pauses, and the sync map that ties each sentence to its clip.

use crate::audio::{Codec, Pcm, SAMPLE_RATE, TrailingSound, fade};
use crate::error::{ErrorKind, Result};
use crate::plan::SectionPlan;
use crate::script::{Boundary, ParagraphRef};
use crate::store::{Recording, RecordingStatus};
use rawr_tts::Alignment;

/// Audio kept before the first aligned character, in milliseconds.
const MARGIN_BEFORE_MS: u32 = 50;
/// Audio kept after the last aligned character, in milliseconds.
const MARGIN_AFTER_MS: u32 = 100;
/// Audio kept after the start of a trailing silence, in milliseconds.
const TRAILING_MARGIN_MS: u32 = 50;
/// Fade at each edge of a recording: 5 ms.
const FADE_SAMPLES: usize = 240;
/// Silence at the start and end of every chapter, in milliseconds.
const LEAD_IN_MS: u32 = 500;
const LEAD_OUT_MS: u32 = 1_000;

/// Silence before a batch, by what comes before it, in milliseconds.
fn pause_ms(boundary: Boundary) -> u32 {
    match boundary {
        Boundary::SectionStart => LEAD_IN_MS,
        Boundary::Heading => 1_000,
        Boundary::Paragraph => 450,
        Boundary::SceneBreak => 1_750,
    }
}

/// A chapter's joined audio and its sync map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterAudio {
    /// [`Section::number`](rawr_source::Section::number).
    pub section: u32,
    pub pcm: Pcm,
    /// Back to back: the first begins at 0, each ends where the next begins,
    /// and the last ends at the end of the audio.
    pub units: Vec<SyncUnit>,
}

/// One clip of the sync map, in samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncUnit {
    pub target: UnitRef,
    pub begin: u64,
    pub end: u64,
}

/// The text a sync unit highlights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitRef {
    Heading,
    /// A sentence of a paragraph: indices into `Script::blocks` and `Paragraph::sentences`.
    Sentence {
        block: usize,
        sentence: usize,
    },
}

/// Where one recording's kept audio sits in the chapter.
struct Placement {
    trim_start: usize,
    trim_end: usize,
    output_start: usize,
}

impl Placement {
    /// The chapter sample of a sample in the recording's own audio.
    fn to_output(&self, sample: usize) -> u64 {
        (self.output_start + sample.clamp(self.trim_start, self.trim_end) - self.trim_start) as u64
    }
}

/// Join a section's recordings into one chapter.
///
/// `recordings` holds one recording for each planned batch, in order. Export
/// never pays: a missing, mismatched or unaligned recording is an error.
///
/// # Errors
///
/// - [`MissingRecording`](ErrorKind::MissingRecording) if a batch has no usable recording
/// - [`MissingAlignment`](ErrorKind::MissingAlignment) if a recording has no alignment
/// - codec errors from decoding
pub fn assemble(section: &SectionPlan, recordings: &[Recording], codec: &dyn Codec) -> Result<ChapterAudio> {
    let mut output: Vec<i16> = Vec::new();
    let mut units: Vec<SyncUnit> = Vec::new();
    for (index, (planned, batch)) in section.batches.iter().zip(&section.script.batches).enumerate() {
        let recording = recordings
            .get(index)
            .filter(|recording| {
                recording.meta.key == planned.key.as_str() && recording.meta.status == RecordingStatus::Ok
            })
            .ok_or_else(|| exn::Exn::new(ErrorKind::MissingRecording(planned.key.to_string())))?;
        let alignment = recording
            .meta
            .alignment()
            .filter(|alignment| !alignment.characters.is_empty())
            .ok_or_else(|| exn::Exn::new(ErrorKind::MissingAlignment(planned.key.to_string())))?;
        let pcm = codec.decode(&recording.audio)?;
        if pcm.sample_rate != SAMPLE_RATE {
            exn::bail!(ErrorKind::UnsupportedAudio(format!("PCM at {} Hz", pcm.sample_rate)));
        }
        let (trim_start, trim_end) = trim(&pcm, &alignment);
        output.extend(std::iter::repeat_n(0, samples(pause_ms(batch.boundary))));
        let placement = Placement {
            trim_start,
            trim_end,
            output_start: output.len(),
        };
        output.extend(fade(&pcm.samples[trim_start..trim_end], FADE_SAMPLES));
        let at_character = |character: usize| {
            let timing = alignment.characters[character.min(alignment.characters.len() - 1)];
            placement.to_output(samples(timing.start_ms))
        };
        for reference in &batch.paragraphs {
            match *reference {
                ParagraphRef::Heading { offset } => units.push(unit(UnitRef::Heading, at_character(offset))),
                ParagraphRef::Block { index: block, offset } => {
                    let paragraph = section
                        .script
                        .paragraph(*reference)
                        .ok_or_else(|| exn::Exn::new(ErrorKind::MissingRecording(planned.key.to_string())))?;
                    for (sentence, range) in paragraph.sentence_chars().into_iter().enumerate() {
                        units.push(unit(UnitRef::Sentence { block, sentence }, at_character(offset + range.start)));
                    }
                },
            }
        }
    }
    output.extend(std::iter::repeat_n(0, samples(LEAD_OUT_MS)));
    let total = output.len() as u64;
    if let Some(first) = units.first_mut() {
        first.begin = 0;
    }
    for index in 1..units.len() {
        if units[index].begin <= units[index - 1].begin {
            units[index].begin = units[index - 1].begin + 1;
        }
    }
    for index in 0..units.len() {
        units[index].end = units.get(index + 1).map_or(total, |next| next.begin);
    }
    Ok(ChapterAudio {
        section: section.script.section,
        pcm: Pcm::new(output, SAMPLE_RATE),
        units,
    })
}

fn unit(target: UnitRef, begin: u64) -> SyncUnit {
    SyncUnit { target, begin, end: begin }
}

fn samples(milliseconds: u32) -> usize {
    (u64::from(milliseconds) * u64::from(SAMPLE_RATE) / 1_000) as usize
}

/// The kept part of a recording: first character − 50 ms to last character + 100 ms,
/// cut earlier at a trailing silence that is followed by a stray sound.
fn trim(pcm: &Pcm, alignment: &Alignment) -> (usize, usize) {
    let length = pcm.len();
    let characters = &alignment.characters;
    let first_start = samples(characters[0].start_ms);
    let last_end = samples(characters[characters.len() - 1].end_ms).min(length);
    let last_letter_end = characters
        .iter()
        .rposition(|timing| timing.character.is_alphanumeric())
        .map_or(last_end, |index| samples(characters[index].end_ms))
        .min(length);
    let start = first_start.saturating_sub(samples(MARGIN_BEFORE_MS));
    let mut end = (last_end + samples(MARGIN_AFTER_MS)).min(length);
    if let Some(silence) = TrailingSound::default().find(&pcm.samples, pcm.sample_rate, last_letter_end) {
        end = end.min(silence + samples(TRAILING_MARGIN_MS));
    }
    (start.min(end), end)
}
