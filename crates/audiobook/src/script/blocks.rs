use super::{Block, Paragraph, SourceRange, TextPosition, sentences};
use scraper::{ElementRef, Html, Node};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

const BLOCK_ELEMENTS: &[&str] = &["p", "div", "blockquote", "li", "h1", "h2", "h3", "h4", "h5", "h6"];
/// A paragraph made only of these (and whitespace) is a scene break.
const BREAK_SYMBOLS: &str = "*~-=_#•·◇◆○●+—–※";

/// Splits a section's XHTML body into cleaned paragraphs (with sentences and
/// their source positions) and scene breaks.
///
/// Consecutive scene breaks collapse to one, and scene breaks at the start
/// and end are removed.
pub(super) fn blocks(body_xhtml: &str) -> Vec<Block> {
    let fragment = Html::parse_fragment(body_xhtml);
    let mut collector = Collector::default();
    collector.walk(fragment.root_element());
    collector.flush();
    let mut blocks = collector.blocks;
    blocks.dedup_by(|a, b| *a == Block::SceneBreak && *b == Block::SceneBreak);
    while blocks.first() == Some(&Block::SceneBreak) {
        blocks.remove(0);
    }
    while blocks.last() == Some(&Block::SceneBreak) {
        blocks.pop();
    }
    blocks
}

/// NFC, whitespace runs collapsed to one space, trimmed.
pub(super) fn clean(text: &str) -> String {
    let text: String = text.nfc().collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

enum Piece {
    Text { node: usize, text: String },
    LineBreak,
}

/// Where one character of cleaned text came from.
#[derive(Clone, Copy)]
struct CharSource {
    start: TextPosition,
    end: TextPosition,
}

#[derive(Clone, Copy)]
enum Separator {
    Space(CharSource),
    LineBreak(CharSource),
}

#[derive(Default)]
struct Collector {
    blocks: Vec<Block>,
    buffer: Vec<Piece>,
    centred: bool,
    /// Text nodes seen so far, in document order.
    nodes: usize,
}

impl Collector {
    fn walk(&mut self, element: ElementRef<'_>) {
        for child in element.children() {
            match child.value() {
                Node::Text(text) => {
                    self.buffer.push(Piece::Text { node: self.nodes, text: text.to_string() });
                    self.nodes += 1;
                },
                Node::Element(value) => {
                    let Some(child) = ElementRef::wrap(child) else {
                        continue;
                    };
                    let name = value.name();
                    if BLOCK_ELEMENTS.contains(&name) {
                        self.flush();
                        self.centred = value.attr("align").is_some_and(|align| align.eq_ignore_ascii_case("center"));
                        self.walk(child);
                        self.flush();
                        self.centred = false;
                    } else if name == "hr" {
                        self.flush();
                        self.blocks.push(Block::SceneBreak);
                    } else if name == "br" {
                        self.buffer.push(Piece::LineBreak);
                    } else {
                        self.walk(child);
                    }
                },
                _ => {},
            }
        }
    }

    fn flush(&mut self) {
        let (text, map) = clean_pieces(&std::mem::take(&mut self.buffer));
        if text.is_empty() {
            return;
        }
        let only_symbols = text.chars().all(|c| c.is_whitespace() || BREAK_SYMBOLS.contains(c));
        let centred_without_letters = self.centred && !text.chars().any(char::is_alphabetic);
        self.blocks.push(if only_symbols || centred_without_letters {
            Block::SceneBreak
        } else {
            let sentences = sentences::sentences(&text);
            let sources = sentences
                .iter()
                .map(|range| {
                    let first = text[..range.start].chars().count();
                    let last = first + text[range.clone()].chars().count() - 1;
                    SourceRange {
                        start: map[first].start,
                        end: map[last].end,
                    }
                })
                .collect();
            Block::Paragraph(Paragraph { text, sentences, sources })
        });
    }
}

/// Clean text from pieces, grapheme by grapheme, keeping each character's
/// source: NFC, whitespace runs become one space, a line break (with the
/// whitespace around it) becomes one newline, and the ends are trimmed.
/// NFC never changes grapheme boundaries, so every character maps to the
/// one source grapheme it came from.
fn clean_pieces(pieces: &[Piece]) -> (String, Vec<CharSource>) {
    let mut text = String::new();
    let mut map: Vec<CharSource> = Vec::new();
    let mut pending: Option<Separator> = None;
    for piece in pieces {
        match piece {
            Piece::LineBreak => {
                if let Some(last) = map.last() {
                    let at = CharSource { start: last.end, end: last.end };
                    pending = Some(Separator::LineBreak(at));
                }
            },
            Piece::Text { node, text: raw } => {
                for (offset, grapheme) in raw.grapheme_indices(true) {
                    let source = CharSource {
                        start: TextPosition { node: *node, offset },
                        end: TextPosition {
                            node: *node,
                            offset: offset + grapheme.len(),
                        },
                    };
                    if grapheme.chars().all(char::is_whitespace) {
                        if !map.is_empty() && pending.is_none() {
                            pending = Some(Separator::Space(source));
                        }
                        continue;
                    }
                    match pending.take() {
                        Some(Separator::Space(at)) => {
                            text.push(' ');
                            map.push(at);
                        },
                        Some(Separator::LineBreak(at)) => {
                            text.push('\n');
                            map.push(at);
                        },
                        None => {},
                    }
                    for character in grapheme.nfc() {
                        text.push(character);
                        map.push(source);
                    }
                }
            },
        }
    }
    (text, map)
}
