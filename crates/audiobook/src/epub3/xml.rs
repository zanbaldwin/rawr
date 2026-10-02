//! XML escaping, with the same rules as rawr-xhtml's serialiser, so text
//! written again comes out byte for byte as rawr-xhtml wrote it.

/// Escape text content: `&`, `<`, `>`, and the no-break space as `&#160;`.
pub(super) fn text(value: &str) -> String {
    escape(value, false)
}

/// Escape a double-quoted attribute value: as [`text`], plus `"`.
pub(super) fn attribute(value: &str) -> String {
    escape(value, true)
}

fn escape(value: &str, attribute: bool) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' if attribute => escaped.push_str("&quot;"),
            '\u{00A0}' => escaped.push_str("&#160;"),
            other => escaped.push(other),
        }
    }
    escaped
}
