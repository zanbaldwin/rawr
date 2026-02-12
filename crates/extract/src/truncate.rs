//! Utilities for truncating HTML documents for efficient extraction.

pub const ESTIMATED_HEADER_SIZE_BYTES: usize = 12 * 1024;

/// Truncates an HTML string to approximately `max_bytes` while ensuring
/// the cut point is at a safe boundary (not mid-tag or mid-entity).
/// This is useful for extracting metadata from very large HTML files,
/// since all required metadata is typically in the first ~10KB.
///
/// # Arguments
///
/// * `html` - The HTML string to truncate
/// * `max_bytes` - The maximum number of bytes to keep
///
/// # Returns
///
/// A string slice of the input up to approximately `max_bytes`, truncated
/// at a safe-ish boundary.
///
/// # Examples
///
/// ```rust
/// use rawr_extract::safe_html_truncate;
/// let html = "<div>Hello World</div>";
/// // Will truncate at a safe boundary, not mid-tag
/// assert_eq!(safe_html_truncate(html, 10).len(), 10);
/// assert_eq!(safe_html_truncate(html, 18).len(), 16)
/// ```
pub fn safe_html_truncate(html: &str, max_bytes: usize) -> &str {
    if html.len() <= max_bytes {
        return html;
    }
    // Start from max_bytes and search backward for a safe cut point
    let mut end = max_bytes;
    // Ensure we don't cut in the middle of a UTF-8 character
    while end > 0 && !html.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        return "";
    }
    let candidate = &html[..end];
    let open_tag_match = candidate.rfind('<');
    let close_tag_match = candidate.rfind('>');
    if let Some(open_tag_position) = open_tag_match
        && close_tag_match.map(|gt| gt < open_tag_position).unwrap_or(true)
    {
        // We're inside a HTML tag, cut before the '<'
        return &candidate[..open_tag_position];
    }
    let start_entity_match = candidate.rfind('&');
    let end_entity_match = candidate.rfind(';');
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
        let html = "<div>Hello</div>";
        assert_eq!(safe_html_truncate(html, 100), html);
    }

    #[test]
    fn truncates_at_tag_boundary() {
        let html = "<div>Hello</div><span>World</span>";
        let result = safe_html_truncate(html, 20);
        // Should cut at the end of </div>
        assert!(result.ends_with(">"));
        assert!(!result.contains("<span"));
    }

    #[test]
    fn does_not_cut_mid_tag() {
        let html = "<div class=\"test\">Content</div>";
        let result = safe_html_truncate(html, 10);
        // Should not cut inside the opening tag
        assert!(result.is_empty() || result.ends_with(">") || !result.contains("<div"), "Result was: {}", result);
    }

    #[test]
    fn handles_entities() {
        let html = "<p>Hello &amp; World</p>";
        let result = safe_html_truncate(html, 12);
        // Should not cut in the middle of &amp;
        assert!(!result.ends_with("&"));
        assert!(!result.ends_with("&a"));
        assert!(!result.ends_with("&am"));
        assert!(!result.ends_with("&amp"));
    }

    #[test]
    fn handles_utf8() {
        let html = "<p>Hello 世界</p>";
        let result = safe_html_truncate(html, 12);
        // Should not panic and should be valid UTF-8
        assert!(result.is_ascii() || result.chars().count() > 0);
    }

    #[test]
    fn empty_string() {
        assert_eq!(safe_html_truncate("", 100), "");
    }

    #[test]
    fn zero_max_bytes() {
        let html = "<div>Hello</div>";
        let result = safe_html_truncate(html, 0);
        assert!(result.is_empty());
    }
}
