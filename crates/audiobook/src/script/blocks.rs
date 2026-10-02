use super::{Block, InlineStyle, Paragraph, Run};
use scraper::{ElementRef, Html, Node};
use unicode_normalization::UnicodeNormalization;

const BLOCK_ELEMENTS: &[&str] = &["p", "div", "blockquote", "li", "h1", "h2", "h3", "h4", "h5", "h6"];
/// A paragraph made only of these (and whitespace) is a scene break.
const BREAK_SYMBOLS: &str = "*~-=_#•·◇◆○●+—–※";

/// Splits a section's XHTML body into cleaned paragraphs and scene breaks.
///
/// Consecutive scene breaks collapse to one, and scene breaks at the start
/// and end are removed. Sentence ranges are left empty for the caller.
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

#[derive(Default)]
struct Collector {
    blocks: Vec<Block>,
    buffer: Vec<Run>,
    styles: Vec<InlineStyle>,
    centred: bool,
}

impl Collector {
    fn walk(&mut self, element: ElementRef<'_>) {
        for child in element.children() {
            match child.value() {
                Node::Text(text) => self.push(text),
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
                        self.push(" ");
                    } else if let Some(style) = InlineStyle::from_element(name) {
                        self.styles.push(style);
                        self.walk(child);
                        self.styles.pop();
                    } else {
                        self.walk(child);
                    }
                },
                _ => {},
            }
        }
    }

    fn push(&mut self, text: &str) {
        match self.buffer.last_mut() {
            Some(run) if run.styles == self.styles => run.text.push_str(text),
            _ => self.buffer.push(Run {
                text: text.to_string(),
                styles: self.styles.clone(),
            }),
        }
    }

    fn flush(&mut self) {
        let runs = clean_runs(&self.buffer);
        self.buffer.clear();
        let text: String = runs.iter().map(|run| run.text.as_str()).collect();
        if text.is_empty() {
            return;
        }
        let only_symbols = text.chars().all(|c| c.is_whitespace() || BREAK_SYMBOLS.contains(c));
        let centred_without_letters = self.centred && !text.chars().any(char::is_alphabetic);
        self.blocks.push(if only_symbols || centred_without_letters {
            Block::SceneBreak
        } else {
            Block::Paragraph(Paragraph { text, runs, sentences: Vec::new() })
        });
    }
}

/// [`clean`] across runs: a space between two runs goes to the end of the earlier run.
fn clean_runs(raw: &[Run]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut pending_space = false;
    for run in raw {
        for character in run.text.nfc() {
            if character.is_whitespace() {
                pending_space = !runs.is_empty();
                continue;
            }
            if pending_space && let Some(last) = runs.last_mut() {
                last.text.push(' ');
            }
            pending_space = false;
            match runs.last_mut() {
                Some(last) if last.styles == run.styles => last.text.push(character),
                _ => runs.push(Run {
                    text: character.to_string(),
                    styles: run.styles.clone(),
                }),
            }
        }
    }
    runs
}
