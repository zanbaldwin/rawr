//! Page content shared by rawr's EPUB renderers: the chapter heading rule and
//! the AO3 title page. Requires the `metadata` feature flag.

use rawr_extract::models::Metadata;

/// The heading of a chapter, or `None` for a single-chapter work, whose only
/// chapter has no heading.
///
/// AO3 chapter headings usually already carry a "Chapter N" prefix; one is
/// only prepended when the title lacks it.
pub fn chapter_heading(number: u32, title: Option<&str>, multi_chapter: bool) -> Option<String> {
    if !multi_chapter {
        return None;
    }
    let prefix = format!("Chapter {number}");
    Some(match title {
        Some(title)
            if title.strip_prefix(&prefix).is_some_and(|rest| rest.is_empty() || rest.starts_with([':', ' '])) =>
        {
            title.to_string()
        },
        Some(title) => format!("{prefix}: {title}"),
        None => prefix,
    })
}

/// Build the title page body: work title, byline, summary and a small
/// metadata list. Warnings and the full tag list intentionally stay in the
/// OPF metadata only.
pub fn title_page_body(meta: &Metadata) -> String {
    let mut body = String::from("<div class=\"title-page\">\n");
    body.push_str(&format!("<h1 class=\"work-title\">{}</h1>\n", xml_escape(&meta.title)));
    let byline = if meta.authors.is_empty() {
        "Anonymous".to_string()
    } else {
        meta.authors.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
    };
    body.push_str(&format!("<p class=\"byline\">by {}</p>\n", xml_escape(&byline)));
    if let Some(summary) = &meta.summary {
        body.push_str("<div class=\"summary\">\n");
        for paragraph in summary.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
            body.push_str(&format!("<p>{}</p>\n", xml_escape(paragraph)));
        }
        body.push_str("</div>\n");
    }
    body.push_str("<dl class=\"work-meta\">\n");
    let mut item = |term: &str, detail: String| {
        body.push_str(&format!("<dt>{term}</dt><dd>{}</dd>\n", xml_escape(&detail)));
    };
    if !meta.fandoms.is_empty() {
        item("Fandom", meta.fandoms.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "));
    }
    for series in &meta.series {
        item("Series", series.to_string());
    }
    if let Some(rating) = &meta.rating {
        item("Rating", rating.to_string());
    }
    item("Words", meta.words.to_string());
    if meta.chapters.written > 1 {
        item("Chapters", meta.chapters.to_string());
    }
    item("Published", meta.published.to_string());
    if meta.last_modified != meta.published {
        item("Updated", meta.last_modified.to_string());
    }
    body.push_str("</dl>\n</div>");
    body
}

/// Escape text for XML text nodes and double-quoted attribute values.
pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
