//! What is spoken, and how it is grouped for narration.
//!
//! A [`Script`] is built from one [`Section`]. It holds the cleaned text in
//! display order ([`Block`]s, for the EPUB) and in narration order
//! ([`NarrationBatch`]es, one per text-to-speech request). Summaries and notes
//! are never spoken.
//!
//! The script is provider-neutral. Providers clean the text they send (for
//! example ElevenLabs and square brackets), and they must keep the character
//! count so that alignment indices still match the offsets here.

mod batches;
mod blocks;
mod sentences;

use crate::error::{ErrorKind, Result};
use rawr_source::Section;
use std::ops::Range;

/// Version of the rules that turn a section into spoken text.
///
/// Bump it whenever a rule changes the output text, so that recordings made
/// under older rules are not mistaken for current ones.
///
/// - 1: the rules from purr (a `<br>` is a space).
/// - 2: a `<br>`, with the whitespace around it, is a newline.
pub const SCRIPT_RECIPE: u32 = 2;

/// Joins paragraphs in a batch. One newline, not a blank line: whitespace is billed.
pub const PARAGRAPH_SEPARATOR: &str = "\n";

/// Marks an element whose text is never spoken (its text nodes still count).
pub const SPEAK_ATTRIBUTE: &str = "data-rawr-speak";
/// Marks an inline element whose spoken text is exactly one sentence.
pub const SENTENCE_ATTRIBUTE: &str = "data-rawr-sentence";
/// Names the voice role of a block element's paragraphs.
pub const VOICE_ATTRIBUTE: &str = "data-rawr-voice";

/// Options for building a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptOptions {
    /// Maximum characters in one batch, from the text-to-speech provider. A
    /// single longer paragraph still forms one batch.
    pub max_batch_chars: usize,
    /// Honour the `data-rawr-*` markers ([`SPEAK_ATTRIBUTE`],
    /// [`SENTENCE_ATTRIBUTE`], [`VOICE_ATTRIBUTE`]). Only sources that write
    /// them set this, so other XHTML is never read differently.
    pub markers: bool,
}

/// The spoken form of one section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    /// [`Section::number`].
    pub section: u32,
    /// CRC32 of the spoken text (heading and paragraphs, joined by [`PARAGRAPH_SEPARATOR`]).
    pub seed: u32,
    /// The spoken heading, if any. One sync unit.
    pub heading: Option<Paragraph>,
    /// The body in display order.
    pub blocks: Vec<Block>,
    /// The heading and body in narration order.
    pub batches: Vec<NarrationBatch>,
}

/// A paragraph or a scene break.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(Paragraph),
    SceneBreak,
}

/// A cleaned paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    /// The spoken text.
    pub text: String,
    /// Sync units (sentences): byte ranges in [`text`](Self::text), in order, not overlapping.
    pub sentences: Vec<Range<usize>>,
    /// Where each sentence is in the section's XHTML, one per sentence. Empty
    /// for the spoken heading, which is not part of the body.
    pub sources: Vec<SourceRange>,
    /// The words of each sentence (split at whitespace): byte ranges in
    /// [`text`](Self::text), one list per sentence.
    pub words: Vec<Vec<Range<usize>>>,
    /// Where each word is in the section's XHTML, like [`sources`](Self::sources).
    pub word_sources: Vec<Vec<SourceRange>>,
    /// The voice role ([`VOICE_ATTRIBUTE`]); `None` is the default voice.
    pub voice: Option<String>,
}

/// A sentence's place in the section's XHTML body, from its first source
/// grapheme to the end of its last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceRange {
    pub start: TextPosition,
    pub end: TextPosition,
}

/// A position in the section's XHTML body.
///
/// `node` counts every text node in document order, as
/// `scraper::Html::parse_fragment(body_xhtml)` parses it; `offset` is a byte
/// offset in that node's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextPosition {
    pub node: usize,
    pub offset: usize,
}

/// The text of one text-to-speech request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrationBatch {
    /// Paragraph texts joined by [`PARAGRAPH_SEPARATOR`].
    pub text: String,
    /// What comes before this batch; it sets the pause before it.
    pub boundary: Boundary,
    /// The paragraphs in this batch, in order.
    pub paragraphs: Vec<ParagraphRef>,
    /// The voice role of every paragraph in this batch ([`Paragraph::voice`]).
    pub voice: Option<String>,
}

/// What comes before a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Boundary {
    SectionStart,
    Heading,
    Paragraph,
    SceneBreak,
}

/// A paragraph in a batch, with its offset in the batch text in **characters**
/// (provider alignment is per character).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParagraphRef {
    Heading { offset: usize },
    Block { index: usize, offset: usize },
}

impl Script {
    /// Build the script of one section.
    ///
    /// `heading` is the spoken heading, given by the caller so that it matches
    /// the displayed heading. It is cleaned like body text; empty means none.
    ///
    /// # Errors
    ///
    /// [`EmptySection`](ErrorKind::EmptySection) if the section has no paragraphs.
    pub fn new(section: &Section, heading: Option<&str>, options: &ScriptOptions) -> Result<Self> {
        let heading = heading.map(blocks::clean).filter(|text| !text.is_empty()).map(|text| {
            let sentences: Vec<_> = std::iter::once(0..text.len()).collect();
            Paragraph {
                words: sentences::words(&text, &sentences),
                word_sources: Vec::new(),
                sentences,
                sources: Vec::new(),
                voice: None,
                text,
            }
        });
        let blocks = blocks::blocks(&section.body_xhtml, options.markers);
        if !blocks.iter().any(|block| matches!(block, Block::Paragraph(_))) {
            exn::bail!(ErrorKind::EmptySection(section.number));
        }
        let batches = batches::batches(heading.as_ref(), &blocks, options.max_batch_chars);
        Ok(Self {
            section: section.number,
            seed: seed(heading.as_ref(), &blocks),
            heading,
            blocks,
            batches,
        })
    }

    /// The paragraph a [`ParagraphRef`] points at.
    pub fn paragraph(&self, reference: ParagraphRef) -> Option<&Paragraph> {
        match reference {
            ParagraphRef::Heading { .. } => self.heading.as_ref(),
            ParagraphRef::Block { index, .. } => match self.blocks.get(index) {
                Some(Block::Paragraph(paragraph)) => Some(paragraph),
                _ => None,
            },
        }
    }
}

impl Paragraph {
    /// [`sentences`](Self::sentences) as character ranges, to match provider alignment.
    pub fn sentence_chars(&self) -> Vec<Range<usize>> {
        self.sentences
            .iter()
            .map(|range| {
                let start = self.text[..range.start].chars().count();
                start..start + self.text[range.clone()].chars().count()
            })
            .collect()
    }
}

impl Paragraph {
    /// [`words`](Self::words) as character ranges, to match provider alignment.
    pub fn word_chars(&self) -> Vec<Vec<Range<usize>>> {
        self.words
            .iter()
            .map(|words| {
                words
                    .iter()
                    .map(|range| {
                        let start = self.text[..range.start].chars().count();
                        start..start + self.text[range.clone()].chars().count()
                    })
                    .collect()
            })
            .collect()
    }
}

fn seed(heading: Option<&Paragraph>, blocks: &[Block]) -> u32 {
    let paragraphs = blocks.iter().filter_map(|block| match block {
        Block::Paragraph(paragraph) => Some(paragraph),
        Block::SceneBreak => None,
    });
    let mut hasher = crc32fast::Hasher::new();
    for (index, paragraph) in heading.into_iter().chain(paragraphs).enumerate() {
        if index > 0 {
            hasher.update(PARAGRAPH_SEPARATOR.as_bytes());
        }
        hasher.update(paragraph.text.as_bytes());
    }
    hasher.finalize()
}
