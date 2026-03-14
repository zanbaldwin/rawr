use super::Stats;
use crate::consts;
use crate::error::{ErrorKind, Result};
use crate::models::{Fandom, Language, Rating, SeriesPosition, Tag, TagKind, Warning};
use exn::{OptionExt, ResultExt};
use scraper::{ElementRef, Html};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct Datalist<'a> {
    list: HashMap<String, ElementRef<'a>>,
}

/* ================== *\
|  Datalist Internals  |
\* ================== */

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
        if dts.len() != dds.len() {
            tracing::debug!(dts = dts.len(), dds = dds.len(), "dt/dd count mismatch in datalist");
        }
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

/* =============== *\
|  Datalist Public  |
\* =============== */

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
        let positions: Vec<_> = consts::SERIES_POSITION_REGEX.captures_iter(&dd_text).collect();
        let anchors: Vec<_> = dd
            .select(&consts::ANCHOR_SELECTOR)
            .filter_map(|anchor| {
                let caps = consts::SERIES_URL_REGEX.captures(anchor.value().attr("href")?)?;
                let id: u64 = caps.get(1)?.as_str().parse().ok()?;
                let name = anchor.text().collect::<String>().trim().to_string();
                Some((id, name))
            })
            .collect();
        if positions.len() != anchors.len() {
            tracing::warn!(
                positions = positions.len(),
                anchors = anchors.len(),
                "series position/anchor count mismatch"
            );
        }
        let mut seen_ids = HashSet::new();
        anchors
            .into_iter()
            .zip(positions)
            .filter_map(|((id, name), capture)| {
                if !seen_ids.insert(id) {
                    return None;
                }
                let position = Some(capture)
                    .and_then(|cap| cap.get(1))
                    .and_then(|m| m.as_str().replace(',', "").parse().ok())
                    .unwrap_or(1);
                Some(SeriesPosition { id, name, position })
            })
            .collect()
    }

    pub fn rating(&self) -> Result<Option<Rating>> {
        self.extract_text(&["Rating"])
            .map(|s| s.parse::<Rating>().or_raise(|| ErrorKind::ParseError { field: "rating", value: s }))
            .transpose()
    }

    pub fn warnings(&self) -> Vec<Warning> {
        self.extract_link_texts(&["Warning", "Warnings", "Archive Warning", "Archive Warnings"])
            .into_iter()
            .filter_map(|text| match text.as_str().parse() {
                Ok(w) => Some(w),
                Err(_) => {
                    tracing::warn!(warning = %text, "unknown archive warning");
                    None
                },
            })
            .collect()
    }

    pub fn tags(&self) -> Vec<Tag> {
        [
            (&["Relationship", "Relationships"] as &[&str], TagKind::Relationship),
            (&["Character", "Characters"], TagKind::Character),
            (&["Additional Tag", "Additional Tags"], TagKind::Freeform),
        ]
        .into_iter()
        .flat_map(|(labels, kind)| self.extract_link_texts(labels).into_iter().map(move |name| Tag { name, kind }))
        .collect()
    }

    pub fn language(&self) -> Language {
        Language::from(self.extract_text(&["Language"]).unwrap_or_else(|| "Unknown".to_string()))
    }
}
