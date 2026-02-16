use crate::error::{Error, ErrorKind};
use crate::models::{FileRow, VersionRow};
use rawr_extract::models as extract;
use rawr_storage::file as storage;

type FileRecord = storage::FileInfo<storage::Processed>;

#[derive(sqlx::FromRow)]
pub(crate) struct JoinRow {
    #[sqlx(flatten)]
    pub(crate) file: FileRow,
    #[sqlx(flatten)]
    pub(crate) version: VersionRow,
}
impl From<JoinRow> for (FileRow, VersionRow) {
    fn from(join: JoinRow) -> Self {
        (join.file, join.version)
    }
}
impl TryFrom<(FileRow, VersionRow)> for JoinRow {
    type Error = Error;
    fn try_from(pair: (FileRow, VersionRow)) -> Result<Self, Self::Error> {
        let (file, version) = pair;
        if file.content_hash != version.content_hash {
            exn::bail!(ErrorKind::Constraint);
        }
        Ok(JoinRow { file, version })
    }
}
impl TryFrom<(&FileRecord, &extract::Version)> for JoinRow {
    type Error = Error;
    fn try_from(pair: (&FileRecord, &extract::Version)) -> Result<Self, Self::Error> {
        let (file, version) = pair;
        if file.content_hash != version.hash {
            exn::bail!(ErrorKind::Constraint);
        }
        let file_row = FileRow::try_from(file)?;
        let version_row = VersionRow::try_from(version)?;
        Ok(JoinRow { file: file_row, version: version_row })
    }
}
impl TryFrom<JoinRow> for (FileRecord, extract::Version) {
    type Error = Error;
    fn try_from(join: JoinRow) -> Result<Self, Self::Error> {
        let file = FileRecord::try_from(join.file)?;
        let version = extract::Version::try_from(join.version)?;
        Ok((file, version))
    }
}

pub(crate) struct OrphanJoinRow {
    pub(crate) file: Option<FileRow>,
    pub(crate) version: VersionRow,
}
impl<'r, R: sqlx::Row> sqlx::FromRow<'r, R> for OrphanJoinRow {
    fn from_row(row: &'r R) -> Result<Self, sqlx::Error> {
        // Content hash is shared between version and file.
        let content_hash = todo!();
        let version: VersionRow = todo!();
        let file = match (col1, col2, col3, ...) {
            (Some(a), Some(b), Some(c), ...) => Some(FileRow { .. }),
            (None, None, None, ...) => None,
            _ => todo!(), // Construct sqlx error.
        };
        Ok(OrphanJoinRow { file, version })
    }
}
