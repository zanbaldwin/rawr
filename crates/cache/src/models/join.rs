use crate::error::{Error, ErrorKind};
use crate::models::{FileRow, VersionRow};
use crate::{File, Version};

#[derive(sqlx::FromRow)]
pub(crate) struct FullJoinRow {
    #[sqlx(flatten)]
    pub(crate) file: FileRow,
    #[sqlx(flatten)]
    pub(crate) version: VersionRow,
}
impl From<FullJoinRow> for (FileRow, VersionRow) {
    fn from(join: FullJoinRow) -> Self {
        (join.file, join.version)
    }
}
impl TryFrom<(FileRow, VersionRow)> for FullJoinRow {
    type Error = Error;
    fn try_from(pair: (FileRow, VersionRow)) -> Result<Self, Self::Error> {
        let (file, version) = pair;
        if file.content_hash != version.content_hash {
            exn::bail!(ErrorKind::Constraint);
        }
        Ok(FullJoinRow { file, version })
    }
}
impl TryFrom<(&File, &Version)> for FullJoinRow {
    type Error = Error;
    fn try_from(pair: (&File, &Version)) -> Result<Self, Self::Error> {
        let (file, version) = pair;
        if file.content_hash != version.hash {
            exn::bail!(ErrorKind::Constraint);
        }
        let file_row = FileRow::try_from(file)?;
        let version_row = VersionRow::try_from(version)?;
        Ok(FullJoinRow { file: file_row, version: version_row })
    }
}
impl TryFrom<FullJoinRow> for (File, Version) {
    type Error = Error;
    fn try_from(join: FullJoinRow) -> Result<Self, Self::Error> {
        let file = File::try_from(join.file)?;
        let version = Version::try_from(join.version)?;
        Ok((file, version))
    }
}

/// Left-join Row Result
///
/// Selecting from "versions LEFT JOIN files" may result in orphaned versions
/// with no related files.
///
/// **Important**: the SELECT order matter with sqlx, as `f.*, v.*` may
/// return a NULL content hash - you must use `v.*, f.*`.
// TODO: That is literally a guess. I haven't written the tests yet and I'm
//       too lazy to set up a database to test it manually.
pub(crate) struct LeftJoinRow {
    pub(crate) file: Option<FileRow>,
    pub(crate) version: VersionRow,
}
impl<'r, R> sqlx::FromRow<'r, R> for LeftJoinRow
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
    i64: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
    Option<String>: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
    Option<i64>: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
{
    fn from_row(row: &'r R) -> Result<Self, sqlx::Error> {
        let version = VersionRow::from_row(row)?;
        let target: Option<String> = row.try_get("target")?;
        let path: Option<String> = row.try_get("path")?;
        let compression: Option<String> = row.try_get("compression")?;
        let file_size: Option<i64> = row.try_get("file_size")?;
        let file_hash: Option<String> = row.try_get("file_hash")?;
        let discovered_at: Option<i64> = row.try_get("discovered_at")?;
        let file = match (target, path, compression, file_size, file_hash, discovered_at) {
            (Some(target), Some(path), Some(compression), Some(file_size), Some(file_hash), Some(discovered_at)) => {
                Some(FileRow {
                    target,
                    path,
                    compression,
                    file_size,
                    file_hash,
                    content_hash: version.content_hash.clone(),
                    discovered_at,
                })
            },
            (None, None, None, None, None, None) => None,
            _ => {
                return Err(sqlx::Error::ColumnDecode {
                    index: "file columns".to_string(),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "LEFT JOIN file columns are partially NULL",
                    )),
                });
            },
        };
        Ok(LeftJoinRow { file, version })
    }
}
impl TryFrom<LeftJoinRow> for (Option<File>, Version) {
    type Error = Error;
    fn try_from(join: LeftJoinRow) -> Result<Self, Self::Error> {
        let file = join.file.map(File::try_from).transpose()?;
        let version = Version::try_from(join.version)?;
        Ok((file, version))
    }
}
impl TryFrom<(Option<&File>, &Version)> for LeftJoinRow {
    type Error = Error;
    fn try_from(pair: (Option<&File>, &Version)) -> Result<Self, Self::Error> {
        let (file, version) = pair;
        if file.is_some_and(|f| f.content_hash != version.hash) {
            exn::bail!(ErrorKind::Constraint);
        }
        let file = file.map(|f| f.try_into()).transpose()?;
        let version = version.try_into()?;
        Ok(LeftJoinRow { file, version })
    }
}
