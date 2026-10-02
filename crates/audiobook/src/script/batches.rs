use super::{Block, Boundary, NarrationBatch, PARAGRAPH_SEPARATOR, Paragraph, ParagraphRef};

/// Groups the heading and paragraphs into narration batches.
///
/// The heading is always a batch of its own, and every scene break closes the
/// current batch. Otherwise a batch closes before a paragraph that would take
/// it over `max_chars`. A paragraph longer than `max_chars` is a batch of its own.
pub(super) fn batches(heading: Option<&Paragraph>, blocks: &[Block], max_chars: usize) -> Vec<NarrationBatch> {
    let mut batches = Vec::new();
    let mut next = Boundary::SectionStart;
    if let Some(heading) = heading {
        batches.push(NarrationBatch {
            text: heading.text.clone(),
            boundary: next,
            paragraphs: vec![ParagraphRef::Heading { offset: 0 }],
        });
        next = Boundary::Heading;
    }
    let mut current = Batch::default();
    for (index, block) in blocks.iter().enumerate() {
        match block {
            Block::SceneBreak => {
                current.close(&mut batches, next);
                next = Boundary::SceneBreak;
            },
            Block::Paragraph(paragraph) => {
                let length = paragraph.text.chars().count();
                if !current.is_empty() && current.length + PARAGRAPH_SEPARATOR.chars().count() + length > max_chars {
                    current.close(&mut batches, next);
                    next = Boundary::Paragraph;
                }
                current.push(index, &paragraph.text, length);
            },
        }
    }
    current.close(&mut batches, next);
    batches
}

#[derive(Default)]
struct Batch {
    text: String,
    length: usize,
    paragraphs: Vec<ParagraphRef>,
}

impl Batch {
    fn is_empty(&self) -> bool {
        self.paragraphs.is_empty()
    }

    fn push(&mut self, index: usize, text: &str, length: usize) {
        if !self.is_empty() {
            self.text.push_str(PARAGRAPH_SEPARATOR);
            self.length += PARAGRAPH_SEPARATOR.chars().count();
        }
        self.paragraphs.push(ParagraphRef::Block { index, offset: self.length });
        self.text.push_str(text);
        self.length += length;
    }

    fn close(&mut self, batches: &mut Vec<NarrationBatch>, boundary: Boundary) {
        if self.is_empty() {
            return;
        }
        let batch = std::mem::take(self);
        batches.push(NarrationBatch {
            text: batch.text,
            boundary,
            paragraphs: batch.paragraphs,
        });
    }
}
