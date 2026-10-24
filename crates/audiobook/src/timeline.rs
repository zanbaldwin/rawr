//! One chapter's audio: a section's recordings, trimmed and joined with
//! pauses, and the sync maps that tie each sentence, and each word, to its clip.

use crate::audio::{Codec, Pcm, SAMPLE_RATE, TrailingSound, fade};
use crate::error::{ErrorKind, Result};
use crate::plan::SectionPlan;
use crate::script::{Boundary, ParagraphRef};
use crate::store::{Recording, RecordingStatus};
use rawr_tts::Alignment;
use std::collections::BTreeMap;

/// Audio kept before the first aligned character, in milliseconds.
const MARGIN_BEFORE_MS: u32 = 50;
/// Audio kept after the last aligned character, in milliseconds.
const MARGIN_AFTER_MS: u32 = 100;
/// Audio kept after the start of a trailing silence, in milliseconds.
const TRAILING_MARGIN_MS: u32 = 50;
/// Fade at each edge of a recording: 5 ms.
const FADE_SAMPLES: usize = 240;
/// The shortest word clip: one millisecond, so that clips rounded down to
/// whole milliseconds never collapse.
const MIN_WORD_SAMPLES: u64 = SAMPLE_RATE as u64 / 1_000;
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
    /// The words of each sentence unit, in order. Each sentence's words are
    /// back to back and cover exactly its clip; a word too close to the one
    /// before it has no clip of its own (it plays within the one before).
    pub words: Vec<SyncUnit>,
    /// How each recording was trimmed, in batch order. Dev and test builds only.
    #[cfg(debug_assertions)]
    pub trims: Vec<TrimReport>,
}

/// How one recording was trimmed, in samples of its own decoded audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrimReport {
    /// The recording's batch key.
    pub key: String,
    /// Length of the decoded recording.
    pub decoded: usize,
    pub start: usize,
    pub end: usize,
    pub end_rule: EndRule,
    /// How far the last timestamp lies past the end of the audio (ElevenLabs' 80 ms grid).
    pub overshoot: usize,
    /// Where the kept audio starts in the chapter.
    pub output_start: usize,
}

/// Why a recording ends where it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndRule {
    /// The last character's end + 100 ms, clamped to the audio.
    LastTimestamp,
    /// The start of a silence after the last letter + 50 ms: a stray sound followed it.
    TrailingSound,
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
    /// A word of a sentence: indices into `Script::blocks`, `Paragraph::sentences`
    /// and `Paragraph::words`.
    Word {
        block: usize,
        sentence: usize,
        word: usize,
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
    let mut word_begins: BTreeMap<(usize, usize), Vec<u64>> = BTreeMap::new();
    #[cfg(debug_assertions)]
    let mut trims = Vec::new();
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
        let trimmed = trim(&pcm, &alignment);
        output.extend(std::iter::repeat_n(0, samples(pause_ms(batch.boundary))));
        let (trim_start, trim_end) = (trimmed.start, trimmed.end);
        let placement = Placement {
            trim_start,
            trim_end,
            output_start: output.len(),
        };
        #[cfg(debug_assertions)]
        trims.push(TrimReport {
            key: planned.key.to_string(),
            decoded: pcm.len(),
            start: trimmed.start,
            end: trimmed.end,
            end_rule: trimmed.end_rule,
            overshoot: trimmed.overshoot,
            output_start: placement.output_start,
        });
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
                    let words = paragraph.word_chars();
                    for (sentence, range) in paragraph.sentence_chars().into_iter().enumerate() {
                        units.push(unit(UnitRef::Sentence { block, sentence }, at_character(offset + range.start)));
                        let begins = words[sentence].iter().map(|word| at_character(offset + word.start)).collect();
                        word_begins.insert((block, sentence), begins);
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
    let words = units
        .iter()
        .flat_map(|unit| match unit.target {
            UnitRef::Sentence { block, sentence } => {
                words(unit, block, sentence, word_begins.get(&(block, sentence)).map_or(&[][..], Vec::as_slice))
            },
            _ => Vec::new(),
        })
        .collect();
    Ok(ChapterAudio {
        section: section.script.section,
        pcm: Pcm::new(output, SAMPLE_RATE),
        units,
        words,
        #[cfg(debug_assertions)]
        trims,
    })
}

/// The word clips of one sentence clip. The first word begins with the
/// sentence; each later word begins at its first character, at least
/// [`MIN_WORD_SAMPLES`] after the word before. A word that would leave less
/// than that before the sentence ends gets no clip.
fn words(sentence_unit: &SyncUnit, block: usize, sentence: usize, begins: &[u64]) -> Vec<SyncUnit> {
    let mut words: Vec<SyncUnit> = Vec::with_capacity(begins.len());
    for (word, &begin) in begins.iter().enumerate() {
        let target = UnitRef::Word { block, sentence, word };
        let Some(previous) = words.last() else {
            words.push(unit(target, sentence_unit.begin));
            continue;
        };
        let begin = begin.max(previous.begin + MIN_WORD_SAMPLES);
        if begin + MIN_WORD_SAMPLES <= sentence_unit.end {
            words.push(unit(target, begin));
        }
    }
    for index in 0..words.len() {
        words[index].end = words.get(index + 1).map_or(sentence_unit.end, |next| next.begin);
    }
    words
}

fn unit(target: UnitRef, begin: u64) -> SyncUnit {
    SyncUnit { target, begin, end: begin }
}

fn samples(milliseconds: u32) -> usize {
    (u64::from(milliseconds) * u64::from(SAMPLE_RATE) / 1_000) as usize
}

#[cfg_attr(not(debug_assertions), allow(dead_code))]
struct Trimmed {
    start: usize,
    end: usize,
    end_rule: EndRule,
    overshoot: usize,
}

/// The kept part of a recording: first character − 50 ms to last character + 100 ms,
/// cut earlier at a trailing silence that is followed by a stray sound.
fn trim(pcm: &Pcm, alignment: &Alignment) -> Trimmed {
    let length = pcm.len();
    let characters = &alignment.characters;
    let first_start = samples(characters[0].start_ms);
    let last_timestamp = samples(characters[characters.len() - 1].end_ms);
    let overshoot = last_timestamp.saturating_sub(length);
    let last_end = last_timestamp.min(length);
    let last_letter_end = characters
        .iter()
        .rposition(|timing| timing.character.is_alphanumeric())
        .map_or(last_end, |index| samples(characters[index].end_ms))
        .min(length);
    let start = first_start.saturating_sub(samples(MARGIN_BEFORE_MS));
    let mut end = (last_end + samples(MARGIN_AFTER_MS)).min(length);
    let mut end_rule = EndRule::LastTimestamp;
    if let Some(silence) = TrailingSound::default().find(&pcm.samples, pcm.sample_rate, last_letter_end) {
        let cut = silence + samples(TRAILING_MARGIN_MS);
        if cut < end {
            end = cut;
            end_rule = EndRule::TrailingSound;
        }
    }
    Trimmed {
        start: start.min(end),
        end,
        end_rule,
        overshoot,
    }
}
