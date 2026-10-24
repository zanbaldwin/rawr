use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// Sentence byte ranges of one cleaned paragraph (UAX #29), trimmed of whitespace.
///
/// A piece whose first letter is lowercase joins the sentence before it:
/// UAX #29 breaks after `?”` in `“Why?” she asks.`, but it is one sentence.
pub(super) fn sentences(text: &str) -> Vec<Range<usize>> {
    let mut sentences: Vec<Range<usize>> = Vec::new();
    for (start, piece) in text.split_sentence_bound_indices() {
        let trimmed = piece.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lead = piece.len() - piece.trim_start().len();
        let range = start + lead..start + lead + trimmed.len();
        let continues = trimmed.chars().find(|c| c.is_alphabetic()).is_some_and(char::is_lowercase);
        match sentences.last_mut() {
            Some(previous) if continues => previous.end = range.end,
            _ => sentences.push(range),
        }
    }
    sentences
}

/// Sentence byte ranges of one cleaned paragraph whose `marked` ranges are
/// one sentence each (trimmed, dropped when empty). Text between them is
/// split by [`sentences`].
pub(super) fn with_marks(text: &str, marked: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut result = Vec::new();
    let unmarked = |range: Range<usize>, result: &mut Vec<Range<usize>>| {
        result.extend(
            sentences(&text[range.clone()])
                .into_iter()
                .map(|sentence| range.start + sentence.start..range.start + sentence.end),
        );
    };
    let mut at = 0;
    for mark in marked {
        if mark.start > at {
            unmarked(at..mark.start, &mut result);
        }
        let piece = &text[mark.clone()];
        let trimmed = piece.trim();
        if !trimmed.is_empty() {
            let start = mark.start + piece.len() - piece.trim_start().len();
            result.push(start..start + trimmed.len());
        }
        at = at.max(mark.end);
    }
    if at < text.len() {
        unmarked(at..text.len(), &mut result);
    }
    result
}

/// The words of each sentence: byte ranges in `text` between whitespace.
pub(super) fn words(text: &str, sentences: &[Range<usize>]) -> Vec<Vec<Range<usize>>> {
    sentences
        .iter()
        .map(|sentence| {
            let mut words = Vec::new();
            let mut start = None;
            for (offset, character) in text[sentence.clone()].char_indices() {
                let at = sentence.start + offset;
                match (character.is_whitespace(), start) {
                    (true, Some(begin)) => {
                        words.push(begin..at);
                        start = None;
                    },
                    (false, None) => start = Some(at),
                    _ => {},
                }
            }
            if let Some(begin) = start {
                words.push(begin..sentence.end);
            }
            words
        })
        .collect()
}
