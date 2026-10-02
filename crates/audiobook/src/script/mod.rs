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
pub const SCRIPT_RECIPE: u32 = 1;

/// Joins paragraphs in a batch. One newline, not a blank line: whitespace is billed.
pub const PARAGRAPH_SEPARATOR: &str = "\n";

/// Options that come from the text-to-speech provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptOptions {
    /// Maximum characters in one batch. A single longer paragraph still forms one batch.
    pub max_batch_chars: usize,
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
    /// The concatenation of [`runs`](Self::runs).
    pub text: String,
    pub runs: Vec<Run>,
    /// Sync units (sentences): byte ranges in [`text`](Self::text), in order, not overlapping.
    pub sentences: Vec<Range<usize>>,
}

/// Text with the same inline formatting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    /// Outermost first.
    pub styles: Vec<InlineStyle>,
}

/// Inline formatting kept for the read-along text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InlineStyle {
    Em,
    Strong,
    I,
    B,
    U,
    S,
    Del,
    Ins,
    Sup,
    Sub,
    Small,
    Code,
    Cite,
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
        let heading = heading.map(blocks::clean).filter(|text| !text.is_empty()).map(|text| Paragraph {
            sentences: std::iter::once(0..text.len()).collect(),
            runs: vec![Run { text: text.clone(), styles: Vec::new() }],
            text,
        });
        let mut blocks = blocks::blocks(&section.body_xhtml);
        for block in &mut blocks {
            if let Block::Paragraph(paragraph) = block {
                paragraph.sentences = sentences::sentences(&paragraph.text);
            }
        }
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

impl InlineStyle {
    /// The style for an XHTML element name, if it is kept.
    pub fn from_element(name: &str) -> Option<Self> {
        Some(match name {
            "em" => Self::Em,
            "strong" => Self::Strong,
            "i" => Self::I,
            "b" => Self::B,
            "u" => Self::U,
            "s" => Self::S,
            "del" => Self::Del,
            "ins" => Self::Ins,
            "sup" => Self::Sup,
            "sub" => Self::Sub,
            "small" => Self::Small,
            "code" => Self::Code,
            "cite" => Self::Cite,
            _ => return None,
        })
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
