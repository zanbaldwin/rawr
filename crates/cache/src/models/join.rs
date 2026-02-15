use crate::error::{Error, ErrorKind};
use crate::models::{FileRow, VersionRow};
use rawr_extract::models as extract;
use rawr_storage::file as storage;

type FileRecord = storage::FileInfo<storage::Processed>;

#[derive(sqlx::FromRow)]
pub(crate) struct JoinRow {
    pub(crate) target: String,
    pub(crate) path: String,
    pub(crate) compression: String,
    pub(crate) file_size: i64,
    pub(crate) file_hash: String,
    pub(crate) content_hash: String,
    pub(crate) discovered_at: i64,
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
impl From<JoinRow> for (FileRow, VersionRow) {
    fn from(join: JoinRow) -> Self {
        let file = FileRow {
            target: join.target,
            path: join.path,
            compression: join.compression,
            file_size: join.file_size,
            file_hash: join.file_hash,
            content_hash: join.content_hash.clone(),
            discovered_at: join.discovered_at,
        };
        let version = VersionRow {
            content_hash: join.content_hash,
            content_crc32: join.content_crc32,
            work_id: join.work_id,
            content_size: join.content_size,
            title: join.title,
            authors: join.authors,
            fandoms: join.fandoms,
            series: join.series,
            chapters_written: join.chapters_written,
            chapters_total: join.chapters_total,
            words: join.words,
            summary: join.summary,
            rating: join.rating,
            warnings: join.warnings,
            lang: join.lang,
            published_on: join.published_on,
            last_modified: join.last_modified,
            tags: join.tags,
            extracted_at: join.extracted_at,
        };
        (file, version)
    }
}
impl TryFrom<(FileRow, VersionRow)> for JoinRow {
    type Error = Error;
    fn try_from(pair: (FileRow, VersionRow)) -> Result<Self, Self::Error> {
        let (file, version) = pair;
        if file.content_hash != version.content_hash {
            exn::bail!(ErrorKind::Constraint);
        }
        Ok(JoinRow {
            target: file.target,
            path: file.path,
            compression: file.compression,
            file_size: file.file_size,
            file_hash: file.file_hash,
            discovered_at: file.discovered_at,
            content_hash: file.content_hash,
            content_crc32: version.content_crc32,
            work_id: version.work_id,
            content_size: version.content_size,
            title: version.title,
            authors: version.authors,
            fandoms: version.fandoms,
            series: version.series,
            chapters_written: version.chapters_written,
            chapters_total: version.chapters_total,
            words: version.words,
            summary: version.summary,
            rating: version.rating,
            warnings: version.warnings,
            lang: version.lang,
            published_on: version.published_on,
            last_modified: version.last_modified,
            tags: version.tags,
            extracted_at: version.extracted_at,
        })
    }
}

impl TryFrom<(&FileRecord, &extract::Version)> for JoinRow {
    type Error = Error;
    fn try_from(pair: (&FileRecord, &extract::Version)) -> Result<Self, Self::Error> {
        let (file, version) = pair;
        let file_row = FileRow::try_from(file)?;
        let version_row = VersionRow::try_from(version)?;
        JoinRow::try_from((file_row, version_row))
    }
}
impl TryFrom<JoinRow> for (FileRecord, extract::Version) {
    type Error = Error;
    fn try_from(row: JoinRow) -> Result<Self, Self::Error> {
        let (file_row, version_row) = row.into();
        let file = FileRecord::try_from(file_row)?;
        let version = extract::Version::try_from(version_row)?;
        Ok((file, version))
    }
}
