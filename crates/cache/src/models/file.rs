use crate::error::{Error, ErrorKind};
use exn::{OptionExt, ResultExt};
use rawr_compress::Compression;
use rawr_storage::file::{self as storage, Processed};
use std::path::PathBuf;
use time::UtcDateTime;

#[derive(sqlx::FromRow)]
pub(crate) struct FileRow {
    target: String,
    path: String,
    compression: String,
    file_size: i64,
    file_hash: String,
    content_hash: String,
    discovered_at: i64,
}
impl TryFrom<&storage::FileInfo<Processed>> for FileRow {
    type Error = Error;
    fn try_from(file: &storage::FileInfo<Processed>) -> Result<Self, Self::Error> {
        Ok(Self {
            target: file.target.clone(),
            path: file.path.to_str().ok_or_raise(|| ErrorKind::InvalidData("path"))?.to_string(),
            compression: file.compression.to_string(),
            file_size: i64::try_from(file.size).or_raise(|| ErrorKind::InvalidData("file size"))?,
            file_hash: file.file_hash.clone(),
            content_hash: file.content_hash.clone(),
            discovered_at: file.discovered_at.unix_timestamp(),
        })
    }
}
impl TryFrom<FileRow> for storage::FileInfo<Processed> {
    type Error = Error;
    fn try_from(row: FileRow) -> Result<Self, Self::Error> {
        let meta = storage::FileMeta {
            target: row.target,
            path: PathBuf::from(row.path),
            compression: row
                .compression
                .parse::<Compression>()
                .or_raise(|| ErrorKind::InvalidData("compression format"))?,
            size: u64::try_from(row.file_size).or_raise(|| ErrorKind::InvalidData("file size"))?,
            discovered_at: UtcDateTime::from_unix_timestamp(row.discovered_at)
                .or_raise(|| ErrorKind::InvalidData("discovery date"))?,
        };
        Ok(meta.with_file_hash(row.file_hash).with_content_hash(row.content_hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawr_storage::file::FileInfo;

    #[test]
    fn test_row_to_model() {
        let discovery = UtcDateTime::now();
        let row = FileRow {
            target: "local".to_string(),
            path: "winnie-the-pooh/12345-teatime-cookbook.html.bz2".to_string(),
            file_size: 1024,
            compression: "bzip2".to_string(),
            file_hash: "6f1b17063da8508541eb76dac260748a2d815c2c88b27cefb6205c90ae16fef5".to_string(),
            content_hash: "692ed948ccd76c2230efe90175a519a3092b1862ab049704b7221738e56028ca".to_string(),
            discovered_at: discovery.unix_timestamp(),
        };
        let model = FileInfo::try_from(row).unwrap();
        assert_eq!(model.compression, Compression::Bzip2);
        // Converting to a Unix timestamp (measured in seconds) inherently strips the nanoseconds component.
        assert_eq!(model.discovered_at, discovery.replace_nanosecond(0).unwrap());
    }

    #[test]
    fn test_model_to_row() {
        let model = FileInfo::new(
            "local".to_string(),
            PathBuf::from("winnie-the-pooh/12345-teatime-cookbook.html.gz"),
            1024,
            UtcDateTime::now(),
            Compression::Gzip,
        )
        .with_file_hash("6f1b17063da8508541eb76dac260748a2d815c2c88b27cefb6205c90ae16fef5")
        .with_content_hash("692ed948ccd76c2230efe90175a519a3092b1862ab049704b7221738e56028ca");
        let row = FileRow::try_from(&model).unwrap();
        assert_eq!(row.compression, "gzip");
    }
}
