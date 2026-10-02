//! A section's XHTML body, as rawr-xhtml wrote it, with sync spans added.
//!
//! Each sentence's span sits under the deepest element that contains both
//! its start and its end. Inline elements between that element and the
//! sentence's edges are closed before the span opens or closes and opened
//! again after it (without their `id`), so the markup stays well-formed and
//! looks the same. Start tags are written lazily, when content first needs
//! them, so splitting never leaves empty elements. Block elements are never
//! split: a sentence never crosses one.

use super::xml;
use crate::script::{Block, Script};
use scraper::{ElementRef, Html, Node};
use std::collections::BTreeMap;

/// HTML void elements, written as `<name/>`.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
];

/// The `id` of each sentence of a script: `{chapter}-s0001`, … in reading order.
pub(super) fn sentence_ids(script: &Script, chapter: &str) -> BTreeMap<(usize, usize), String> {
    let mut ids = BTreeMap::new();
    for (block, paragraph) in script.blocks.iter().enumerate().filter_map(|(index, block)| match block {
        Block::Paragraph(paragraph) => Some((index, paragraph)),
        Block::SceneBreak => None,
    }) {
        for sentence in 0..paragraph.sentences.len() {
            let id = format!("{chapter}-s{:04}", ids.len() + 1);
            ids.insert((block, sentence), id);
        }
    }
    ids
}

enum Event {
    /// Open a span with this `id`, under this many ancestors of the text node.
    Open(String, usize),
    Close,
}

struct Open {
    name: String,
    attributes: Vec<(String, String)>,
    /// The start tag is in the output and not closed yet.
    written: bool,
    /// The start tag was written at least once.
    ever_written: bool,
}

struct Writer {
    out: String,
    stack: Vec<Open>,
    /// Text node index → events at byte offsets, closes before opens at the same offset.
    events: BTreeMap<usize, Vec<(usize, u8, Event)>>,
    /// The level of the open span (number of ancestors it sits under).
    span_level: Option<usize>,
    nodes: usize,
}

/// The body with a `<span id>` around every sentence of the script.
pub(super) fn with_spans(body_xhtml: &str, script: &Script, ids: &BTreeMap<(usize, usize), String>) -> String {
    let fragment = Html::parse_fragment(body_xhtml);
    let mut chains = Vec::new();
    let mut serial = 0;
    chain(fragment.root_element(), &mut Vec::new(), &mut serial, &mut chains);
    let mut events: BTreeMap<usize, Vec<(usize, u8, Event)>> = BTreeMap::new();
    for (block, paragraph) in script.blocks.iter().enumerate().filter_map(|(index, block)| match block {
        Block::Paragraph(paragraph) => Some((index, paragraph)),
        Block::SceneBreak => None,
    }) {
        for (sentence, source) in paragraph.sources.iter().enumerate() {
            let id = ids[&(block, sentence)].clone();
            let (start, end) = (&chains[source.start.node], &chains[source.end.node]);
            let level = start.iter().zip(end).take_while(|(a, b)| a == b).count();
            events.entry(source.start.node).or_default().push((source.start.offset, 1, Event::Open(id, level)));
            events.entry(source.end.node).or_default().push((source.end.offset, 0, Event::Close));
        }
    }
    for list in events.values_mut() {
        list.sort_by_key(|(offset, order, _)| (*offset, *order));
    }
    let mut writer = Writer {
        out: String::with_capacity(body_xhtml.len() + body_xhtml.len() / 8),
        stack: Vec::new(),
        events,
        span_level: None,
        nodes: 0,
    };
    writer.children(fragment.root_element());
    writer.out
}

/// Number every element in document order and record each text node's chain.
fn chain(element: ElementRef<'_>, path: &mut Vec<usize>, serial: &mut usize, chains: &mut Vec<Vec<usize>>) {
    for child in element.children() {
        match child.value() {
            Node::Text(_) => chains.push(path.clone()),
            Node::Element(_) => {
                if let Some(child) = ElementRef::wrap(child) {
                    path.push(*serial);
                    *serial += 1;
                    chain(child, path, serial, chains);
                    path.pop();
                }
            },
            _ => {},
        }
    }
}

impl Writer {
    fn children(&mut self, element: ElementRef<'_>) {
        for child in element.children() {
            match child.value() {
                Node::Text(text) => {
                    let node = self.nodes;
                    self.nodes += 1;
                    self.text(node, text);
                },
                Node::Element(value) => {
                    let Some(child) = ElementRef::wrap(child) else {
                        continue;
                    };
                    let name = value.name().to_string();
                    let attributes =
                        value.attrs().map(|(name, value)| (name.to_string(), value.to_string())).collect::<Vec<_>>();
                    if VOID.contains(&name.as_str()) {
                        self.write_pending();
                        self.out.push('<');
                        self.out.push_str(&name);
                        push_attributes(&mut self.out, &attributes, true);
                        self.out.push_str("/>");
                        continue;
                    }
                    self.write_pending();
                    self.stack.push(Open {
                        name,
                        attributes,
                        written: false,
                        ever_written: false,
                    });
                    self.children(child);
                    let open = self.stack.pop().expect("pushed above");
                    if open.written {
                        self.out.push_str(&format!("</{}>", open.name));
                    } else if !open.ever_written {
                        self.out.push('<');
                        self.out.push_str(&open.name);
                        push_attributes(&mut self.out, &open.attributes, true);
                        self.out.push_str(&format!("></{}>", open.name));
                    }
                },
                _ => {},
            }
        }
    }

    fn text(&mut self, node: usize, text: &str) {
        let events = self.events.remove(&node).unwrap_or_default();
        let mut at = 0;
        for (offset, _, event) in events {
            if offset > at {
                self.write_text(&text[at..offset]);
                at = offset;
            }
            match event {
                Event::Open(id, level) => self.open_span(&id, level),
                Event::Close => self.close_span(),
            }
        }
        if at < text.len() {
            self.write_text(&text[at..]);
        }
    }

    fn write_text(&mut self, text: &str) {
        self.write_pending();
        self.out.push_str(&xml::text(text));
    }

    /// Write the start tags of open elements that content now needs.
    fn write_pending(&mut self) {
        for open in &mut self.stack {
            if !open.written {
                self.out.push('<');
                self.out.push_str(&open.name);
                push_attributes(&mut self.out, &open.attributes, !open.ever_written);
                self.out.push('>');
                open.written = true;
                open.ever_written = true;
            }
        }
    }

    /// Close the written elements deeper than `level`, so a span can open or close there.
    fn close_above(&mut self, level: usize) {
        let from = level.min(self.stack.len());
        for open in self.stack[from..].iter_mut().rev() {
            if open.written {
                self.out.push_str(&format!("</{}>", open.name));
                open.written = false;
            }
        }
    }

    fn open_span(&mut self, id: &str, level: usize) {
        for open in &mut self.stack[..level] {
            if !open.written {
                self.out.push('<');
                self.out.push_str(&open.name);
                push_attributes(&mut self.out, &open.attributes, !open.ever_written);
                self.out.push('>');
                open.written = true;
                open.ever_written = true;
            }
        }
        self.close_above(level);
        self.out.push_str(&format!("<span id=\"{}\">", xml::attribute(id)));
        self.span_level = Some(level);
    }

    fn close_span(&mut self) {
        let level = self.span_level.take().unwrap_or(0);
        self.close_above(level);
        self.out.push_str("</span>");
    }
}

fn push_attributes(out: &mut String, attributes: &[(String, String)], with_id: bool) {
    for (name, value) in attributes {
        if !with_id && name == "id" {
            continue;
        }
        out.push_str(&format!(" {name}=\"{}\"", xml::attribute(value)));
    }
}
