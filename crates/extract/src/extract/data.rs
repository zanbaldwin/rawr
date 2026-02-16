use super::Stats;
use crate::consts;
use crate::error::{ErrorKind, Result};
use crate::models::{Fandom, Language, Rating, SeriesPosition, Tag, TagKind, Warning};
use ::regex::{Regex, escape as regex_escape};
use exn::{OptionExt, ResultExt};
use scraper::{ElementRef, Html};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct Datalist<'a> {
    list: HashMap<String, ElementRef<'a>>,
}

/// Datalist Internals
impl<'a> Datalist<'a> {
    pub(crate) fn new(document: &'a Html) -> Self {
        Self {
            list: Self::collect_labels(&document.select(&consts::TAGS_DL_SELECTOR).next()),
        }
    }

    fn collect_labels(element: &Option<ElementRef<'a>>) -> HashMap<String, ElementRef<'a>> {
        let Some(element) = element else {
            return HashMap::new();
        };
        let dts: Vec<_> = element.select(&consts::DT_SELECTOR).collect();
        let dds: Vec<_> = element.select(&consts::DD_SELECTOR).collect();
        dts.into_iter()
            .zip(dds)
            .map(|(dt, dd)| (dt.text().collect::<String>().trim().trim_end_matches(':').to_string(), dd))
            .collect()
    }

    fn find_by_label(&self, labels: &[&str]) -> Option<ElementRef<'a>> {
        labels.iter().find_map(|label| self.list.get(*label).copied())
    }

    fn extract_text(&self, labels: &[&str]) -> Option<String> {
        self.find_by_label(labels).map(|dd| dd.text().collect::<String>().trim().to_string())
    }

    fn extract_link_texts(&self, labels: &[&str]) -> Vec<String> {
        let Some(dd) = self.find_by_label(labels) else {
            return Vec::new();
        };
        let mut seen = HashSet::new();
        let mut texts = Vec::new();
        for anchor in dd.select(&consts::ANCHOR_SELECTOR) {
            let text = anchor.text().collect::<String>().trim().to_string();
            if !text.is_empty() && seen.insert(text.clone()) {
                texts.push(text);
            }
        }
        texts
    }
}

/// Datalist Public
impl<'a> Datalist<'a> {
    pub fn stats(&self) -> Result<Stats> {
        Ok(Stats::new(self.extract_text(&["Stats"]).ok_or_raise(|| ErrorKind::MissingField("Stats"))?))
    }

    pub fn fandoms(&self) -> Vec<Fandom> {
        self.extract_link_texts(&["Fandom", "Fandoms"]).into_iter().map(|name| name.into()).collect()
    }

    pub fn series(&self) -> Vec<SeriesPosition> {
        let Some(dd) = self.find_by_label(&["Series"]) else {
            return Vec::new();
        };
        let dd_text = dd.text().collect::<String>();
        let mut series = Vec::new();
        let mut seen_ids = HashSet::new();
        for anchor in dd.select(&consts::ANCHOR_SELECTOR) {
            let Some(href) = anchor.value().attr("href") else {
                continue;
            };
            let Some(captures) = consts::SERIES_URL_REGEX.captures(href) else {
                continue;
            };
            let series_id: u64 = match captures.get(1).unwrap().as_str().parse() {
                Ok(id) => id,
                Err(_) => continue,
            };
            // Deduplicate
            if seen_ids.contains(&series_id) {
                continue;
            }
            seen_ids.insert(series_id);
            let series_name = anchor.text().collect::<String>().trim().to_string();
            // Extract position: look for "Part N of {series_name}"
            // TODO: Can this be done via a lazy Regex?
            let position_pattern = format!(r"Part\s+(\d{{1,3}}(?:,?\d{{3}})*)\s+of\s+{}", regex_escape(&series_name));
            let position = Regex::new(&position_pattern)
                .ok()
                .and_then(|re| re.captures(&dd_text))
                .and_then(|cap| cap.get(1))
                .and_then(|m| m.as_str().replace(',', "").parse().ok())
                .unwrap_or(1);
            series.push(SeriesPosition {
                id: series_id,
                name: series_name,
                position,
            });
        }
        series
    }

    pub fn rating(&self) -> Result<Option<Rating>> {
        Ok(if let Some(s) = self.extract_text(&["Rating"]) {
            Some(s.parse::<Rating>().or_raise(|| ErrorKind::ParseError { field: "rating", value: s })?)
        } else {
            None
        })
    }

    pub fn warnings(&self) -> Vec<Warning> {
        self.extract_link_texts(&["Warning", "Warnings", "Archive Warning", "Archive Warnings"])
            .into_iter()
            .filter_map(|text| text.as_str().parse().ok())
            .collect()
    }

    pub fn tags(&self) -> Vec<Tag> {
        let mut tags = Vec::new();
        // Relationships
        for name in self.extract_link_texts(&["Relationship", "Relationships"]) {
            tags.push(Tag { name, kind: TagKind::Relationship });
        }
        // Characters
        for name in self.extract_link_texts(&["Character", "Characters"]) {
            tags.push(Tag { name, kind: TagKind::Character });
        }
        // Freeform/Additional tags
        for name in self.extract_link_texts(&["Additional Tag", "Additional Tags"]) {
            tags.push(Tag { name, kind: TagKind::Freeform });
        }
        tags
    }

    pub fn language(&self) -> Language {
        Language::from(self.extract_text(&["Language"]).unwrap_or_else(|| "Unknown".to_string()))
    }
}
