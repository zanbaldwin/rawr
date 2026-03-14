use std::collections::HashMap;

use scraper::ElementRef;

use super::Extractor;
use crate::consts;
use crate::models::ChapterContent;

impl Extractor {
    /// Extracts chapter content from the full AO3 HTML document.
    ///
    /// Unlike [`metadata()`](Self::metadata), this requires the complete
    /// (non-truncated) HTML to access chapter body content. Returns chapters
    /// in document order.
    ///
    /// For multi-chapter works, identifies chapters by their `div.meta.group`
    /// header elements within `div#chapters`. For single-chapter works, falls
    /// back to the direct `div.userstuff` child of `div#chapters`.
    pub fn chapters(&self) -> Vec<ChapterContent> {
        let meta_groups: Vec<_> = self.document.select(&consts::CHAPTER_META_GROUP_SELECTOR).collect();
        if meta_groups.is_empty() { self.single_chapter() } else { self.multi_chapter(&meta_groups) }
    }

    fn multi_chapter(&self, meta_groups: &[ElementRef<'_>]) -> Vec<ChapterContent> {
        let bodies: Vec<_> = self.document.select(&consts::SINGLE_CHAPTER_SELECTOR).collect();
        let endnotes = self.endnotes_by_id();
        meta_groups
            .iter()
            .enumerate()
            .map(|(i, meta)| {
                let number = (i as u32) + 1;
                let title = meta
                    .select(&consts::CHAPTER_HEADING_SELECTOR)
                    .next()
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .filter(|s| !s.is_empty());
                let body_html = bodies.get(i).map(|el| el.inner_html()).unwrap_or_default();
                let summary = Self::extract_labeled_blockquote(meta, "Chapter Summary");
                let author_notes = Self::extract_labeled_blockquote(meta, "Chapter Notes");
                let end_notes = endnotes.get(format!("endnotes{number}").as_str()).and_then(|el| {
                    el.select(&consts::CHAPTER_NOTES_BLOCKQUOTE_SELECTOR).next().map(|bq| bq.inner_html())
                });
                ChapterContent {
                    number,
                    title,
                    summary,
                    body_html,
                    author_notes,
                    end_notes,
                }
            })
            .collect()
    }

    fn single_chapter(&self) -> Vec<ChapterContent> {
        self.document
            .select(&consts::SINGLE_CHAPTER_SELECTOR)
            .next()
            .map(|el| {
                let author_notes = self
                    .document
                    .select(&consts::PREFACE_META_SELECTOR)
                    .next()
                    .and_then(|meta| Self::extract_labeled_blockquote(&meta, "Notes"));
                let end_notes = self
                    .document
                    .select(&consts::AFTERWORD_ENDNOTES_SELECTOR)
                    .next()
                    .and_then(|endnotes| {
                        endnotes
                            .select(&consts::CHAPTER_NOTES_BLOCKQUOTE_SELECTOR)
                            .next()
                            .map(|bq| bq.inner_html())
                    });
                vec![ChapterContent {
                    number: 1,
                    title: None,
                    summary: None,
                    body_html: el.inner_html(),
                    author_notes,
                    end_notes,
                }]
            })
            .unwrap_or_default()
    }

    /// Extracts a `<blockquote class="userstuff">` from within a chapter's
    /// `div.meta.group` that follows a `<p>` with the given label text.
    ///
    /// AO3 uses `<p>Chapter Summary</p>` and `<p>Chapter Notes</p>` as labels
    /// before their respective blockquotes. This method walks direct children
    /// to match the correct blockquote by its preceding label.
    fn extract_labeled_blockquote(meta_group: &ElementRef<'_>, label: &str) -> Option<String> {
        let mut last_p_text = String::new();
        for child in meta_group.child_elements() {
            match child.value().name() {
                "p" => last_p_text = child.text().collect::<String>().trim().to_string(),
                "blockquote" if child.value().classes().any(|c| c == "userstuff") && last_p_text == label => {
                    return Some(child.inner_html());
                },
                _ => {},
            }
        }
        None
    }

    /// Collects chapter end notes elements into a map keyed by their `id`
    /// attribute (e.g. `"endnotes3"` -> element).
    ///
    /// Only matches `div[id^="endnotes"]` that are direct children of
    /// `div#chapters`, excluding work-level end notes in `div#afterword`.
    fn endnotes_by_id(&self) -> HashMap<&str, ElementRef<'_>> {
        self.document
            .select(&consts::CHAPTER_ENDNOTES_SELECTOR)
            .filter_map(|el| el.value().id().map(|id| (id, el)))
            .collect()
    }
}
