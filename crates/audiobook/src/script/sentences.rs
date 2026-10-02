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
