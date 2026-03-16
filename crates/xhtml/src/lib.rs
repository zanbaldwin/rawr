//! XHTML serializer for converting html5ever-parsed DOM trees into valid XHTML.
//!
//! Implements the [`html5ever::serialize::Serializer`] trait to produce XHTML
//! output from scraper's tree walk. Handles void element self-closing, proper
//! XML entity escaping, and optional element filtering.

use html5ever::serialize::{AttrRef, Serialize, Serializer, TraversalScope};
use html5ever::{QualName, local_name, ns};
use scraper::ElementRef;
use std::io::{Result as IoResult, Write};

/// Serialize an element's inner content as valid XHTML.
#[must_use]
pub fn to_xhtml(element: &ElementRef<'_>) -> String {
    serialize_inner(element, None)
}

/// Serialize an element's inner content as valid XHTML for offline use.
///
/// Strips external asset elements (scripts, images, media, iframes, etc). This
/// is the appropriate choice for ebook or archival rendering where external
/// resources are unavailable.
#[must_use]
pub fn to_offline_xhtml(element: &ElementRef<'_>) -> String {
    serialize_inner(element, Some(offline_filter))
}

type FilterFn = fn(&QualName) -> bool;
/// Pre-built element filter for offline/ebook rendering.
///
/// Returns `true` for elements that should be stripped: scripts, stylesheets,
/// images, media, iframes, and other elements that reference external
/// resources.
fn offline_filter(name: &QualName) -> bool {
    name.ns == ns!(html)
        && matches!(
            name.local,
            local_name!("script")
                | local_name!("noscript")
                | local_name!("link")
                | local_name!("meta")
                | local_name!("base")
                | local_name!("style")
                | local_name!("iframe")
                | local_name!("object")
                | local_name!("embed")
                | local_name!("img")
                | local_name!("video")
                | local_name!("audio")
                | local_name!("canvas")
        )
}

fn serialize_inner(element: &ElementRef<'_>, filter: Option<FilterFn>) -> String {
    let mut buf = Vec::new();
    let mut serializer = XhtmlSerializer::new(&mut buf, filter);
    element
        .serialize(&mut serializer, TraversalScope::ChildrenOnly(None))
        // Safety: will only fail if you run out of memory, in which case the
        // process will panic and you're fucked anyway.
        .expect("XHTML serialization to Vec<u8> should not fail");
    // This I'm not quite sure about... the HTML is user-generated, and
    // untrustworthy. XHTML _should_ be UTF8, but does html5ever guarantee that?
    // Eh, we can fix this if I ever encounter panics...
    String::from_utf8(buf).expect("XHTML output should be valid UTF-8")
}

/// HTML void elements that must be self-closed with `/>` in XHTML.
///
/// Note: some entries overlap with [`offline_filter`] (e.g. `img`, `embed`,
/// `link`, `meta`, `base`). When filtering is active, those elements are
/// skipped before the void check is reached — the overlap is harmless.
fn is_void(name: &QualName) -> bool {
    name.ns == ns!(html)
        && matches!(
            name.local,
            local_name!("area")
                | local_name!("base")
                | local_name!("br")
                | local_name!("col")
                | local_name!("embed")
                | local_name!("hr")
                | local_name!("img")
                | local_name!("input")
                | local_name!("link")
                | local_name!("meta")
                | local_name!("param")
                | local_name!("source")
                | local_name!("track")
                | local_name!("wbr")
        )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ElemAction {
    Write,
    Skip,
}

struct XhtmlSerializer<W: Write> {
    writer: W,
    filter: Option<FilterFn>,
    stack: Vec<ElemAction>,
}
impl<W: Write> XhtmlSerializer<W> {
    fn new(writer: W, filter: Option<FilterFn>) -> Self {
        Self {
            writer,
            filter,
            stack: vec![ElemAction::Write],
        }
    }

    fn is_skipping(&self) -> bool {
        self.stack.last() == Some(&ElemAction::Skip)
    }

    /// Escapes text for valid XHTML output.
    ///
    /// `<` and `>` are escaped unconditionally — XML requires `<` to be escaped
    /// even inside attribute values. `"` is only escaped in attribute mode since
    /// all attributes use double-quote delimiters.
    ///
    /// Note: `'` (`&apos;`) is intentionally not escaped because this serializer
    /// always uses double-quoted attribute values, so single quotes are safe.
    fn write_escaped(&mut self, text: &str, attr_mode: bool) -> IoResult<()> {
        let mut buf = [0u8; 4];
        for c in text.chars() {
            self.writer.write_all(match c {
                '&' => b"&amp;",
                '"' if attr_mode => b"&quot;",
                '<' => b"&lt;",
                '>' => b"&gt;",
                '\u{00A0}' => b"&#160;",
                c => c.encode_utf8(&mut buf).as_bytes(),
            })?;
        }
        Ok(())
    }

    fn write_attrs(&mut self, attrs: &[AttrRef<'_>]) -> IoResult<()> {
        for (name, value) in attrs {
            self.writer.write_all(b" ")?;
            match name.ns {
                ns!() => {},
                ns!(xml) => self.writer.write_all(b"xml:")?,
                ns!(xmlns) => {
                    if name.local != local_name!("xmlns") {
                        self.writer.write_all(b"xmlns:")?;
                    }
                },
                ns!(xlink) => self.writer.write_all(b"xlink:")?,
                _ => {},
            }
            self.writer.write_all(name.local.as_bytes())?;
            self.writer.write_all(b"=\"")?;
            self.write_escaped(value, true)?;
            self.writer.write_all(b"\"")?;
        }
        Ok(())
    }
}
impl<W: Write> Serializer for XhtmlSerializer<W> {
    /// Note: only the local element name is written (no namespace prefix).
    /// This is intentional — AO3 body fragments do not contain namespaced
    /// elements, so `xml:`/`svg:` prefixes are not needed for this use case.
    fn start_elem<'a, AttrIter>(&mut self, name: QualName, attrs: AttrIter) -> IoResult<()>
    where
        AttrIter: Iterator<Item = AttrRef<'a>>,
    {
        if self.is_skipping() {
            self.stack.push(ElemAction::Skip);
            return Ok(());
        }
        if self.filter.is_some_and(|f| f(&name)) {
            self.stack.push(ElemAction::Skip);
            return Ok(());
        }
        let attrs: Vec<_> = attrs.collect();
        self.writer.write_all(b"<")?;
        self.writer.write_all(name.local.as_bytes())?;
        self.write_attrs(&attrs)?;
        if is_void(&name) {
            self.writer.write_all(b"/>")?;
            self.stack.push(ElemAction::Skip);
        } else {
            self.writer.write_all(b">")?;
            self.stack.push(ElemAction::Write);
        }
        Ok(())
    }

    fn end_elem(&mut self, name: QualName) -> IoResult<()> {
        match self.stack.pop() {
            Some(ElemAction::Write) => {
                self.writer.write_all(b"</")?;
                self.writer.write_all(name.local.as_bytes())?;
                self.writer.write_all(b">")
            },
            _ => Ok(()),
        }
    }

    fn write_text(&mut self, text: &str) -> IoResult<()> {
        if self.is_skipping() {
            return Ok(());
        }
        self.write_escaped(text, false)
    }

    fn write_comment(&mut self, _text: &str) -> IoResult<()> {
        Ok(())
    }

    fn write_doctype(&mut self, _name: &str) -> IoResult<()> {
        Ok(())
    }

    fn write_processing_instruction(&mut self, _target: &str, _data: &str) -> IoResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scraper::{Html, Selector};

    /// Parse HTML as a document and run `to_xhtml` on the first child of `<body>`.
    fn xhtml(html: &str) -> String {
        let doc = Html::parse_document(html);
        let sel = Selector::parse("body > *").unwrap();
        let el = doc.select(&sel).next().unwrap();
        to_xhtml(&el)
    }

    /// Parse HTML as a document and run `to_offline_xhtml` on the first child of `<body>`.
    fn offline(html: &str) -> String {
        let doc = Html::parse_document(html);
        let sel = Selector::parse("body > *").unwrap();
        let el = doc.select(&sel).next().unwrap();
        to_offline_xhtml(&el)
    }

    #[test]
    fn plain_text_content() {
        assert_eq!(xhtml("<p>hello world</p>"), "hello world");
    }

    #[test]
    fn nested_elements() {
        assert_eq!(xhtml("<div><p>text</p></div>"), "<p>text</p>");
    }

    #[test]
    fn attributes_preserved() {
        assert_eq!(
            xhtml(r#"<div><a href="/works" class="tag">link</a></div>"#),
            r#"<a class="tag" href="/works">link</a>"#,
        );
    }

    #[test]
    fn empty_element() {
        assert_eq!(xhtml("<div></div>"), "");
    }

    #[test]
    fn empty_span_round_trips() {
        assert_eq!(xhtml("<div><span></span></div>"), "<span></span>");
    }

    #[test]
    fn br_self_closed() {
        assert_eq!(xhtml("<div><br></div>"), "<br/>");
    }

    #[test]
    fn hr_self_closed() {
        assert_eq!(xhtml("<div><hr></div>"), "<hr/>");
    }

    #[test]
    fn input_self_closed() {
        assert_eq!(xhtml("<div><input></div>"), "<input/>");
    }

    #[test]
    fn void_element_with_attributes() {
        assert_eq!(xhtml(r#"<div><input type="text" name="q"/></div>"#), r#"<input name="q" type="text"/>"#,);
    }

    #[test]
    fn ampersand_in_text() {
        assert_eq!(xhtml("<p>A &amp; B</p>"), "A &amp; B");
    }

    #[test]
    fn lt_gt_in_text() {
        assert_eq!(xhtml("<p>1 &lt; 2 &gt; 0</p>"), "1 &lt; 2 &gt; 0");
    }

    #[test]
    fn quote_in_attribute() {
        assert_eq!(
            xhtml(r#"<div><span title="say &quot;hi&quot;">x</span></div>"#),
            r#"<span title="say &quot;hi&quot;">x</span>"#,
        );
    }

    #[test]
    fn nbsp_encoded() {
        // \u{00A0} is a non-breaking space; the parser preserves it from &nbsp;
        assert_eq!(xhtml("<p>\u{00A0}</p>"), "&#160;");
    }

    #[test]
    fn lt_in_attribute_escaped() {
        assert_eq!(xhtml(r#"<div><span data-x="a&lt;b">y</span></div>"#), r#"<span data-x="a&lt;b">y</span>"#,);
    }

    #[test]
    fn single_quote_in_attribute_not_escaped() {
        assert_eq!(xhtml(r#"<div><span title="it's">z</span></div>"#), r#"<span title="it's">z</span>"#,);
    }

    #[test]
    fn script_stripped() {
        assert_eq!(offline("<div><script>alert(1)</script></div>"), "");
    }

    #[test]
    fn style_stripped() {
        assert_eq!(offline("<div><style>body{}</style></div>"), "");
    }

    #[test]
    fn img_stripped() {
        assert_eq!(offline(r#"<div><img src="a.png"/></div>"#), "");
    }

    #[test]
    fn iframe_stripped() {
        assert_eq!(offline(r#"<div><iframe src="x"></iframe></div>"#), "");
    }

    #[test]
    fn non_filtered_elements_preserved() {
        assert_eq!(offline("<div><p>keep</p></div>"), "<p>keep</p>");
        assert_eq!(offline("<div><span>ok</span></div>"), "<span>ok</span>");
        assert_eq!(offline(r##"<div><a href="#">link</a></div>"##), r##"<a href="#">link</a>"##,);
    }

    #[test]
    fn nested_content_inside_filtered_stripped() {
        assert_eq!(offline("<div><script><p>hidden</p></script></div>"), "",);
    }

    #[test]
    fn mixed_filtered_and_non_filtered() {
        let result = offline(r#"<div><p>keep</p><script>drop</script><span>also keep</span></div>"#);
        assert_eq!(result, "<p>keep</p><span>also keep</span>");
    }

    #[test]
    fn to_xhtml_preserves_img() {
        let html = r#"<div><img src="photo.jpg"/></div>"#;
        assert_eq!(xhtml(html), r#"<img src="photo.jpg"/>"#);
    }

    #[test]
    fn to_offline_xhtml_strips_img() {
        let html = r#"<div><img src="photo.jpg"/></div>"#;
        assert_eq!(offline(html), "");
    }

    #[test]
    fn to_xhtml_preserves_script() {
        let html = "<div><script>var x=1;</script></div>";
        assert_eq!(xhtml(html), "<script>var x=1;</script>");
    }

    #[test]
    fn to_offline_xhtml_strips_script() {
        let html = "<div><script>var x=1;</script></div>";
        assert_eq!(offline(html), "");
    }

    #[test]
    fn empty_input() {
        assert_eq!(xhtml("<div></div>"), "");
    }

    #[test]
    fn deeply_nested() {
        assert_eq!(
            xhtml("<div><div><div><div><p>deep</p></div></div></div></div>"),
            "<div><div><div><p>deep</p></div></div></div>",
        );
    }

    #[test]
    fn comments_stripped() {
        assert_eq!(xhtml("<div><!-- comment -->text</div>"), "text");
    }

    #[test]
    fn comments_stripped_offline() {
        assert_eq!(offline("<div><!-- comment -->text</div>"), "text");
    }

    #[test]
    fn awful_user_generated_content() {
        let html5 = r#"<div id=wrapper">
<H1>VERY LOUD HEADING</h1>
<div>
<p>Something, something<br  > &amp; <b class=test>something</b>&hellip;</p>
<p>Unclosed paragraph tag.
</div>
<img src="data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iODAwIiBoZWlnaHQ9IjgwMCIgdmlld0JveD0iMCAwIDMyIDMyIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPjxwYXRoIGQ9Ik0wIDJ2MjJoMzJWMnptMiAyaDI4djE3SDJ6IiBzdHlsZT0ib3BhY2l0eToxO2ZpbGw6IzM3MzczNztmaWxsLW9wYWNpdHk6MTtzdHJva2U6bm9uZSIvPjxwYXRoIHN0eWxlPSJvcGFjaXR5OjE7ZmlsbDojMzczNzM3O2ZpbGwtb3BhY2l0eToxO3N0cm9rZTpub25lIiBkPSJNMTMgMTExMy41Mmg2djZoLTZ6IiB0cmFuc2Zvcm09InRyYW5zbGF0ZSgwIC0xMDkwLjUyKSIvPjxwYXRoIHN0eWxlPSJvcGFjaXR5OjE7ZmlsbDojMzczNzM3O2ZpbGwtb3BhY2l0eToxO3N0cm9rZTpub25lO3N0cm9rZS13aWR0aDoxLjEzMzg5MzM3IiBkPSJNNyAxMTE4LjUyaDE4djJIN3oiIHRyYW5zZm9ybT0idHJhbnNsYXRlKDAgLTEwOTAuNTIpIi8+PHBhdGggc3R5bGU9Im9wYWNpdHk6MTt2ZWN0b3ItZWZmZWN0Om5vbmU7ZmlsbDojMzczNzM3O2ZpbGwtb3BhY2l0eTouMjUwOTgwNDE7c3Ryb2tlOm5vbmU7c3Ryb2tlLXdpZHRoOjI7c3Ryb2tlLWxpbmVjYXA6YnV0dDtzdHJva2UtbGluZWpvaW46YmV2ZWw7c3Ryb2tlLW1pdGVybGltaXQ6NDtzdHJva2UtZGFzaGFycmF5Om5vbmU7c3Ryb2tlLWRhc2hvZmZzZXQ6My4yMDAwMDAwNTtzdHJva2Utb3BhY2l0eToxIiBkPSJNMiAxMDk0LjUyaDI4djE3SDJ6IiB0cmFuc2Zvcm09InRyYW5zbGF0ZSgwIC0xMDkwLjUyKSIvPjwvc3ZnPg==" width=100 height=100 alt='Screen'>
</div>"#;
        let expected = r#"
<h1>VERY LOUD HEADING</h1>
<div>
<p>Something, something<br/> &amp; <b class="test">something</b>…</p>
<p>Unclosed paragraph tag.
</p></div>
        "#;
        assert_eq!(offline(html5).trim(), expected.trim());
    }
}
