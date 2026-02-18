//! Utilities for truncating HTML documents for efficient extraction.

use memchr::memrchr;

pub const ESTIMATED_HEADER_SIZE_BYTES: usize = 12 * 1024;

/// Truncates raw HTML bytes to approximately `max_bytes` while ensuring
/// the cut point is at a safe boundary (not mid-tag or mid-entity).
/// This is useful for extracting metadata from very large HTML files,
/// since all required metadata is typically in the first ~10KB.
///
/// Accepts raw bytes, instead of requiring HTML to be valid UTF-8. The tag and
/// entity markers (`<`, `>`, `&`, `;`) are all ASCII-range bytes, so
/// searching is safe.
///
/// # Arguments
///
/// * `html` - The raw HTML bytes to truncate
/// * `max_bytes` - The maximum number of bytes to keep
///
/// # Returns
///
/// A byte slice of the input up to approximately `max_bytes`, truncated
/// at a safe-ish (ISH!) boundary.
///
/// # Examples
///
/// ```rust
/// use rawr_extract::safe_html_truncate;
/// let html = b"<div>Hello World</div>";
/// // Will truncate at a safe boundary, not mid-tag
/// assert_eq!(safe_html_truncate(html, 10).len(), 10);
/// assert_eq!(safe_html_truncate(html, 18).len(), 16)
/// ```
pub fn safe_html_truncate(html: &[u8], max_bytes: usize) -> &[u8] {
    if html.len() <= max_bytes {
        return html;
    }
    let candidate = &html[..max_bytes];
    let open_tag_match = memrchr(b'<', candidate);
    let close_tag_match = memrchr(b'>', candidate);
    if let Some(open_tag_position) = open_tag_match
        && close_tag_match.map(|gt| gt < open_tag_position).unwrap_or(true)
    {
        // We're inside a HTML tag, cut before the '<'
        return &candidate[..open_tag_position];
    }
    let start_entity_match = memrchr(b'&', candidate);
    let end_entity_match = memrchr(b';', candidate);
    if let Some(amp_pos) = start_entity_match
        && end_entity_match.map(|semi| semi < amp_pos).unwrap_or(true)
    {
        // We're inside a HTML entity, cut before the '&'
        return &candidate[..amp_pos];
    }
    // We're neither inside a HTML tag, or inside a HTML entity.
    // We _should_ be in the middle of text, which is completely fine to truncate.
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_truncation_needed() {
        let html = b"<div>Hello</div>";
        assert_eq!(safe_html_truncate(html, 100), html);
    }

    #[test]
    fn truncates_at_tag_boundary() {
        let html = b"<div>Hello</div><span>World</span>";
        let result = safe_html_truncate(html, 20);
        // Should cut at the end of </div>
        assert!(result.ends_with(b">"));
        assert!(!result.windows(5).any(|w| w == b"<span"));
    }

    #[test]
    fn does_not_cut_mid_tag() {
        let html = b"<div class=\"test\">Content</div>";
        let result = safe_html_truncate(html, 10);
        // Should not cut inside the opening tag
        assert!(result.is_empty() || result.ends_with(b">") || !result.windows(4).any(|w| w == b"<div"));
    }

    #[test]
    fn handles_entities() {
        let html = b"<p>Hello &amp; World</p>";
        let result = safe_html_truncate(html, 12);
        // Should not cut in the middle of &amp;
        assert!(!result.ends_with(b"&"));
        assert!(!result.ends_with(b"&a"));
        assert!(!result.ends_with(b"&am"));
        assert!(!result.ends_with(b"&amp"));
    }

    #[test]
    fn handles_non_utf8() {
        // Latin-1 encoded bytes (not valid UTF-8)
        let html = b"<p>Hello \xe9\xe8\xe0</p>";
        let result = safe_html_truncate(html, 12);
        // Should not panic — operates purely on bytes
        assert!(!result.is_empty());
    }

    #[test]
    fn empty_bytes() {
        assert_eq!(safe_html_truncate(b"", 100), b"");
    }

    #[test]
    fn zero_max_bytes() {
        let html = b"<div>Hello</div>";
        let result = safe_html_truncate(html, 0);
        assert!(result.is_empty());
    }
}
