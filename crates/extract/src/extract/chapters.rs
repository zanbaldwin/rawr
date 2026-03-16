use std::collections::HashMap;

use scraper::ElementRef;

use super::Extractor;
use crate::consts;
use crate::models::ChapterContent;

type SerializeFn = fn(&ElementRef<'_>) -> String;

impl Extractor {
    /// Extracts chapter content from the full AO3 HTML document.
    ///
    /// > Unlike [`metadata()`](Self::metadata), this requires the complete
    /// > (non-truncated) HTML to access chapter body content.
    ///
    /// Multi-chapter works have author and end notes per-chapter, whereas
    /// single-chapter works use the author and end notes of the entire work.
    ///
    /// Extracts the user-generated content as-is from the HTML, and returns
    /// chapters in document order.
    pub fn chapters_html(&self) -> Vec<ChapterContent> {
        self.extract_chapters(|element| element.inner_html())
    }

    /// Extracts chapter content as XHTML
    ///
    /// Same as [`chapters_html()`](Self::chapters_html), but sanitises and
    /// serializes the user content as valid XHTML.
    ///
    /// Performs offline rendering, so external (non-inlined) media is also
    /// filtered out: images, scripts, iframes, etc.
    pub fn chapters_xhtml(&self) -> Vec<ChapterContent> {
        self.extract_chapters(rawr_xhtml::to_offline_xhtml)
    }

    fn extract_chapters(&self, serialize: SerializeFn) -> Vec<ChapterContent> {
        let meta_groups: Vec<_> = self.document.select(&consts::CHAPTER_META_GROUP_SELECTOR).collect();
        if meta_groups.is_empty() {
            self.extract_single_chapter(serialize)
        } else {
            self.extract_multi_chapter(&meta_groups, serialize)
        }
    }

    fn extract_multi_chapter(&self, meta_groups: &[ElementRef<'_>], serialize: SerializeFn) -> Vec<ChapterContent> {
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
                let body_html = bodies.get(i).map(|el| serialize(el)).unwrap_or_default();
                let summary = Self::extract_labeled_blockquote(meta, "Chapter Summary", serialize);
                let author_notes = Self::extract_labeled_blockquote(meta, "Chapter Notes", serialize);
                let end_notes = endnotes.get(format!("endnotes{number}").as_str()).and_then(|el| {
                    el.select(&consts::CHAPTER_NOTES_BLOCKQUOTE_SELECTOR).next().map(|bq| serialize(&bq))
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

    fn extract_single_chapter(&self, serialize: SerializeFn) -> Vec<ChapterContent> {
        self.document
            .select(&consts::SINGLE_CHAPTER_SELECTOR)
            .next()
            .map(|el| {
                let author_notes = self
                    .document
                    .select(&consts::PREFACE_META_SELECTOR)
                    .next()
                    .and_then(|meta| Self::extract_labeled_blockquote(&meta, "Notes", serialize));
                let end_notes =
                    self.document.select(&consts::AFTERWORD_ENDNOTES_SELECTOR).next().and_then(|endnotes| {
                        endnotes.select(&consts::CHAPTER_NOTES_BLOCKQUOTE_SELECTOR).next().map(|bq| serialize(&bq))
                    });
                vec![ChapterContent {
                    number: 1,
                    title: None,
                    summary: None,
                    body_html: serialize(&el),
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
    fn extract_labeled_blockquote(meta_group: &ElementRef<'_>, label: &str, serialize: SerializeFn) -> Option<String> {
        let mut last_p_text = String::new();
        for child in meta_group.child_elements() {
            match child.value().name() {
                "p" => last_p_text = child.text().collect::<String>().trim().to_string(),
                "blockquote" if child.value().classes().any(|c| c == "userstuff") && last_p_text == label => {
                    return Some(serialize(&child));
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
