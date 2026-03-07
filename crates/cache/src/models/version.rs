use crate::Version;
use crate::error::{Error, ErrorKind};
use exn::ResultExt;
use rawr_extract::models as extract;
use serde_json::{from_str as from_json, to_string as to_json};
use time::UtcDateTime;

#[derive(sqlx::FromRow)]
pub(crate) struct VersionRow {
    pub(crate) content_hash: String,
    pub(crate) content_crc32: i64,
    pub(crate) work_id: i64,
    pub(crate) content_size: i64,
    pub(crate) title: String,
    pub(crate) authors: String,
    pub(crate) fandoms: String,
    pub(crate) series: String,
    pub(crate) chapters_written: i64,
    #[sqlx(default)]
    pub(crate) chapters_total: Option<i64>,
    pub(crate) words: i64,
    pub(crate) summary: Option<String>,
    pub(crate) rating: Option<String>,
    pub(crate) warnings: String,
    pub(crate) lang: String,
    pub(crate) published_on: i64,
    pub(crate) last_modified: i64,
    pub(crate) tags: String,
    pub(crate) extracted_at: i64,
}
impl TryFrom<&Version> for VersionRow {
    type Error = Error;
    fn try_from(version: &Version) -> Result<Self, Self::Error> {
        Ok(Self {
            content_hash: version.hash.clone(),
            content_crc32: i64::from(version.crc32),
            work_id: i64::try_from(version.metadata.work_id).or_raise(|| ErrorKind::InvalidData("work id"))?,
            content_size: i64::try_from(version.length).or_raise(|| ErrorKind::InvalidData("content size"))?,
            title: version.metadata.title.clone(),
            authors: to_json(&version.metadata.authors).or_raise(|| ErrorKind::InvalidData("authors"))?,
            fandoms: to_json(&version.metadata.fandoms).or_raise(|| ErrorKind::InvalidData("fandoms"))?,
            series: to_json(&version.metadata.series).or_raise(|| ErrorKind::InvalidData("series"))?,
            chapters_written: i64::from(version.metadata.chapters.written),
            chapters_total: version.metadata.chapters.total.map(i64::from),
            words: i64::try_from(version.metadata.words).or_raise(|| ErrorKind::InvalidData("words"))?,
            summary: version.metadata.summary.as_ref().map(|s| s.to_string()),
            rating: version.metadata.rating.map(|r| r.as_short_str().to_string()),
            warnings: to_json(&version.metadata.warnings).or_raise(|| ErrorKind::InvalidData("warnings"))?,
            lang: version.metadata.language.name.clone(),
            published_on: version.metadata.published.midnight().as_utc().unix_timestamp(),
            last_modified: version.metadata.last_modified.midnight().as_utc().unix_timestamp(),
            tags: to_json(&version.metadata.tags).or_raise(|| ErrorKind::InvalidData("tags"))?,
            extracted_at: version.extracted_at.unix_timestamp(),
        })
    }
}
impl TryFrom<VersionRow> for Version {
    type Error = Error;
    fn try_from(row: VersionRow) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: row.content_hash,
            crc32: u32::try_from(row.content_crc32).or_raise(|| ErrorKind::InvalidData("crc32"))?,
            length: u64::try_from(row.content_size).or_raise(|| ErrorKind::InvalidData("content length"))?,
            metadata: extract::Metadata {
                work_id: u64::try_from(row.work_id).or_raise(|| ErrorKind::InvalidData("work id"))?,
                title: row.title,
                authors: from_json(&row.authors).or_raise(|| ErrorKind::InvalidData("authors"))?,
                fandoms: from_json(&row.fandoms).or_raise(|| ErrorKind::InvalidData("fandoms"))?,
                series: from_json(&row.series).or_raise(|| ErrorKind::InvalidData("series"))?,
                chapters: extract::Chapters::new(
                    u32::try_from(row.chapters_written).or_raise(|| ErrorKind::InvalidData("chapters written"))?,
                    row.chapters_total
                        .map(|c| u32::try_from(c).or_raise(|| ErrorKind::InvalidData("chapters total")))
                        .transpose()?,
                ),
                words: u64::try_from(row.words).or_raise(|| ErrorKind::InvalidData("words"))?,
                rating: row
                    .rating
                    .map(|r| r.parse::<extract::Rating>().or_raise(|| ErrorKind::InvalidData("rating")))
                    .transpose()?,
                warnings: from_json(&row.warnings).or_raise(|| ErrorKind::InvalidData("warnings"))?,
                tags: from_json(&row.tags).or_raise(|| ErrorKind::InvalidData("tags"))?,
                summary: row.summary,
                // Infallible: Language accepts any string.
                language: row.lang.parse::<extract::Language>().unwrap(),
                published: UtcDateTime::from_unix_timestamp(row.published_on)
                    .or_raise(|| ErrorKind::InvalidData("published on date"))?
                    .date(),
                last_modified: UtcDateTime::from_unix_timestamp(row.last_modified)
                    .or_raise(|| ErrorKind::InvalidData("last modified date"))?
                    .date(),
            },
            extracted_at: UtcDateTime::from_unix_timestamp(row.extracted_at)
                .or_raise(|| ErrorKind::InvalidData("extraction date"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawr_extract::models::{self as extract, Metadata, Version};
    use time::{Date, Month, UtcDateTime};

    #[test]
    fn test_row_to_model() {
        let row = VersionRow {
            content_hash: "692ed948ccd76c2230efe90175a519a3092b1862ab049704b7221738e56028ca".to_string(),
            content_crc32: 123,
            work_id: 12345,
            content_size: 1024,
            title: "Winnie the Pooh's Teatime Cookbook".to_string(),
            authors: r#"["aamilne82"]"#.to_string(),
            fandoms: r#"["Winnie-the-Pooh - A. A. Milne"]"#.to_string(),
            series: "[]".to_string(),
            chapters_written: 6,
            chapters_total: Some(6),
            words: 19375,
            summary: None,
            rating: Some("G".to_string()),
            warnings: r#"["NoWarningsApply"]"#.to_string(),
            lang: "English".to_string(),
            published_on: 820450800,
            last_modified: 820450800,
            tags: r#"[{"name":"Piglet (Winnie-the-Pooh)","kind":"Character"}]"#.to_string(),
            extracted_at: 1771177811,
        };
        let model = Version::try_from(row).unwrap();
        assert!(matches!(
            model.metadata.tags.first(),
            Some(extract::Tag {
                name: _,
                kind: extract::TagKind::Character
            })
        ));
    }

    #[test]
    fn test_model_to_row() {
        let published_on = Date::from_calendar_date(1996, Month::January, 1).unwrap();
        let model = Version {
            hash: "692ed948ccd76c2230efe90175a519a3092b1862ab049704b7221738e56028ca".to_string(),
            crc32: 123,
            length: 1024,
            metadata: Metadata {
                work_id: 12345,
                title: "Winnie the Pooh's Teatime Cookbook".to_string(),
                authors: vec![extract::Author {
                    username: "aamilne82".to_string(),
                    pseudonym: None,
                }],
                fandoms: vec![extract::Fandom {
                    name: "Winnie-the-Pooh - A. A. Milne".to_string(),
                }],
                series: vec![],
                chapters: extract::Chapters { written: 6, total: Some(6) },
                words: 19375,
                summary: None,
                rating: Some(extract::Rating::GeneralAudiences),
                warnings: vec![extract::Warning::NoWarningsApply],
                language: extract::Language::new("English"),
                published: published_on,
                last_modified: published_on,
                tags: vec![extract::Tag {
                    name: "Piglet (Winnie-the-Pooh)".to_string(),
                    kind: extract::TagKind::Character,
                }],
            },
            extracted_at: UtcDateTime::now(),
        };
        let row = VersionRow::try_from(&model).unwrap();
        assert_eq!(row.published_on, published_on.midnight().as_utc().unix_timestamp());
    }
}
