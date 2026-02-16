//! Combined repository for File and Version structs.
//!
//! They're tightly coupled: can't have a File without a Version, and there's
//! no point keeping a version if there's no physical file to extract it from
//! (unless for historical record keeping).

use crate::error::{ErrorKind, Result};
use crate::models::{FileRow, FullJoinRow, LeftJoinRow, VersionRow};
use crate::{Database, File, Version};
use exn::{OptionExt, ResultExt};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::Path;

type FileResult = (File, Version);
type OptionalFileResult = (Option<File>, Version);
type VersionResult = (Version, Vec<File>);

/// Result of checking whether a file exists in the cache.
#[derive(Debug, Eq, PartialEq)]
pub enum ExistenceResult {
    /// No file record exists at the given path.
    NotFound,
    /// A file record exists at the given path and the file hash matches.
    ExactMatch(File, Version),
    /// A file record exists at the given path, but the current file hash does
    /// not match what's on record.
    ///
    /// The file could be corrupt, it could have been replaced, it could have
    /// been recompressed to another format. Whatever happened, the current file
    /// record is stale and the file needs to be re-imported to update the cache.
    HashMismatch(File, Version),
    /// The specified file is not recorded in the cache database, but a file
    /// with the same file hash is known about at a different location.
    LocatedElsewhere(File, Version),
}

fn group_files_by_version(rows: Vec<FileResult>) -> Vec<VersionResult> {
    // TODO: Weep at the allocations. Burn this quick and dirty hack to the ground.
    //       I was meant to refactor group_optional_files_by_version before
    //       I got to the point of using it even more, but I got lazy.
    group_optional_files_by_version(rows.into_iter().map(|(f, v)| (Some(f), v)).collect())
}

fn group_optional_files_by_version(rows: Vec<OptionalFileResult>) -> Vec<VersionResult> {
    let mut map: HashMap<String, (Version, Vec<File>)> = HashMap::new();
    for (file, version) in rows {
        let entry = map.entry(version.hash.clone()).or_insert_with(|| (version, Vec::new()));
        if let Some(file) = file {
            entry.1.push(file);
        }
    }
    map.into_values().collect()
}

/// Repository for managing File and Version entries in the cache database.
///
/// This repository treats files and versions as a unit. Files track physical
/// locations in storage targets by path, while versions track unique content
/// (identified by BLAKE3 hash of decompressed HTML) and its extracted metadata.
///
/// When `dry_run` is enabled, write operations (inserts, updates, deletes)
/// still validate their inputs but skip the actual database mutation,
/// returning the same values they would on success.
///
/// # Relationships
/// - Many files can reference the same version (duplicate content at different paths)
/// - Files can be using different compression (duplicate version content hash, different file hash)
/// - Many versions can exist for the same work_id (different downloads over time)
/// - Deleting a version cascades to delete all files referencing it
/// - Deleting all files for a version leaves an orphan (cleaned up separately)
#[derive(Debug, Clone)]
pub struct Repository {
    pool: SqlitePool,
    dry_run: bool,
}
impl From<&Database> for Repository {
    fn from(db: &Database) -> Self {
        Self { pool: db.pool().clone(), dry_run: false }
    }
}
impl Repository {
    /// Create a new repository with the given connection pool.
    pub fn new(pool: SqlitePool, dry_run: bool) -> Self {
        Self { pool, dry_run }
    }

    fn sqlx_hates_paths(path: impl AsRef<Path>) -> Result<String> {
        Ok(path.as_ref().to_str().ok_or_raise(|| ErrorKind::InvalidData("path"))?.to_string())
    }

    /* ============== *\
    |  Upsert Methods  |
    \* ============== */

    /// Insert a file and its associated version into the database.
    ///
    /// This performs an atomic upsert of both records in a transaction (if a
    /// version with the same content hash already exists, it is reused; if a
    /// file at the same (target, path) exists, it is replaced).
    ///
    /// Returns [`ErrorKind::Constraint`] if the file's content hash does not
    /// match the version's content hash.
    pub async fn upsert(&self, file: &File, version: &Version) -> Result<()> {
        if file.content_hash != version.hash {
            exn::bail!(ErrorKind::Constraint);
        }
        if self.dry_run {
            return Ok(());
        }
        let version_row = VersionRow::try_from(version)?;
        let file_row = FileRow::try_from(file)?;
        let mut tx = self.pool.begin().await.or_raise(|| ErrorKind::Database)?;
        sqlx::query(include_str!("../queries/upsert_version.sql"))
            .bind(version_row.content_hash)
            .bind(version_row.content_crc32)
            .bind(version_row.work_id)
            .bind(version_row.content_size)
            .bind(version_row.title)
            .bind(version_row.authors)
            .bind(version_row.fandoms)
            .bind(version_row.series)
            .bind(version_row.chapters_written)
            .bind(version_row.chapters_total)
            .bind(version_row.words)
            .bind(version_row.summary)
            .bind(version_row.rating)
            .bind(version_row.warnings)
            .bind(version_row.lang)
            .bind(version_row.published_on)
            .bind(version_row.last_modified)
            .bind(version_row.tags)
            .bind(version_row.extracted_at)
            .execute(&mut *tx)
            .await
            .or_raise(|| ErrorKind::Database)?;
        sqlx::query(include_str!("../queries/upsert_file.sql"))
            .bind(file_row.target)
            .bind(file_row.path)
            .bind(file_row.compression)
            .bind(file_row.file_size)
            .bind(file_row.file_hash)
            .bind(file_row.content_hash)
            .bind(file_row.discovered_at)
            .execute(&mut *tx)
            .await
            .or_raise(|| ErrorKind::Database)?;
        tx.commit().await.or_raise(|| ErrorKind::Database)?;
        Ok(())
    }

    /* ================ *\
    |  Fetching Methods  |
    \* ================ */

    /// Look up a single file record and its associated version by storage
    /// target and relative path.
    ///
    /// Returns `None` if no file is recorded at that location.
    pub async fn get_by_target_path(
        &self,
        target: impl AsRef<str>,
        path: impl AsRef<Path>,
    ) -> Result<Option<FileResult>> {
        let row: Option<FullJoinRow> = sqlx::query_as(include_str!("../queries/get_by_target_path.sql"))
            .bind(target.as_ref())
            .bind(Self::sqlx_hates_paths(path)?)
            .fetch_optional(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        row.map(|r| r.try_into()).transpose()
    }

    /// Look up all file records matching a relative path, regardless of target.
    pub async fn get_by_path_across_targets(&self, path: impl AsRef<Path>) -> Result<Vec<FileResult>> {
        let row: Vec<FullJoinRow> = sqlx::query_as(include_str!("../queries/get_by_path_across_targets.sql"))
            .bind(Self::sqlx_hates_paths(path)?)
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        row.into_iter().map(|r| r.try_into()).collect::<Result<Vec<_>>>()
    }

    /// Get all files and their versions matching a file hash within a storage target.
    ///
    /// The file hash is a BLAKE3 hash of the (compressed) file as stored on
    /// disk. This is useful for detecting if a file's content has changed.
    ///
    /// > **Note:** Multiple files could theoretically have the same file hash
    /// > if they are exact copies (in different paths/targets).
    pub async fn get_by_target_file_hash(
        &self,
        target: impl AsRef<str>,
        file_hash: impl AsRef<str>,
    ) -> Result<Vec<FileResult>> {
        let rows: Vec<FullJoinRow> = sqlx::query_as(include_str!("../queries/get_by_target_file_hash.sql"))
            .bind(target.as_ref())
            .bind(file_hash.as_ref())
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// Get all files and their versions matching a file hash across all storage targets.
    ///
    /// The file hash is a BLAKE3 hash of the (compressed) file as stored on
    /// disk. This is useful for detecting if a file's content has changed.
    ///
    /// > **Note:** Multiple files could theoretically have the same file hash
    /// > if they are exact copies (in different paths/targets).
    pub async fn get_by_file_hash_across_targets(&self, file_hash: impl AsRef<str>) -> Result<Vec<FileResult>> {
        let rows: Vec<FullJoinRow> = sqlx::query_as(include_str!("../queries/get_by_file_hash_across_targets.sql"))
            .bind(file_hash.as_ref())
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// Get a version and all files that reference it by content hash.
    ///
    /// The content hash is a BLAKE3 hash of the decompressed HTML content.
    /// Multiple files may reference the same version if the same content
    /// exists at different paths, with different compression formats, or in different targets.
    pub async fn get_by_content_hash(&self, content_hash: impl AsRef<str>) -> Result<Option<VersionResult>> {
        let content_hash = content_hash.as_ref();
        let rows: Vec<LeftJoinRow> = sqlx::query_as(include_str!("../queries/get_by_content_hash.sql"))
            .bind(content_hash)
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let pairs = rows.into_iter().map(|r| r.try_into()).collect::<Result<Vec<_>>>()?;
        Ok(group_optional_files_by_version(pairs).into_iter().next())
    }

    /// Get all versions and their files for a given AO3 work ID.
    ///
    /// A work may have multiple versions if it was downloaded at different
    /// times (e.g., before and after the author added chapters). Each version
    /// may have multiple files if duplicates exist (possibly across targets).
    ///
    /// Results are sorted by the version comparison algorithm (best/newest first).
    pub async fn get_by_work_id(&self, work_id: u64) -> Result<Vec<VersionResult>> {
        let work_id = i64::try_from(work_id).or_raise(|| ErrorKind::InvalidData("work id"))?;
        let rows: Vec<LeftJoinRow> = sqlx::query_as(include_str!("../queries/get_by_work_id.sql"))
            .bind(work_id)
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        let pairs = rows.into_iter().map(|r| r.try_into()).collect::<Result<Vec<_>>>()?;
        let mut map = group_optional_files_by_version(pairs);
        map.sort_by(|(a, _), (b, _)| b.cmp(a));
        Ok(map)
    }

    /// Get the best version for a work ID and all files referencing it.
    ///
    /// "Best" is determined by the version comparison algorithm in `rawr-extract`,
    /// which considers factors like last modified date, chapter count, and
    /// file size.
    pub async fn get_best_for_work_id(&self, work_id: u64) -> Result<Option<VersionResult>> {
        // TODO: This is NOT more efficient, but it IS easier to implement and
        //       that's all we care about right now.
        Ok(self.get_by_work_id(work_id).await?.into_iter().next())
    }

    // =========================================================================
    // Listing
    // =========================================================================

    /// List all distinct target identifiers that have file records in the database.
    ///
    /// A "target" is a named storage location (e.g., `"local"`, `"s3-backup"`)
    /// under which files are tracked.
    pub async fn list_scanned_targets(&self) -> Result<Vec<String>> {
        let targets: Vec<String> = sqlx::query_scalar(include_str!("../queries/list_scanned_targets.sql"))
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        Ok(targets)
    }

    /// List all versions and their associated files for a target.
    ///
    /// Returns a list of (version, files) tuples. Each version appears once
    /// with all files that reference it within the target.
    pub async fn list_versions_for_target(&self, target: impl AsRef<str>) -> Result<Vec<VersionResult>> {
        let rows: Vec<FullJoinRow> = sqlx::query_as(include_str!("../queries/list_versions_for_target.sql"))
            .bind(target.as_ref())
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        let pairs = rows.into_iter().map(|r| r.try_into()).collect::<Result<Vec<_>>>()?;
        Ok(group_files_by_version(pairs))
    }

    /// List all files for a specific target.
    ///
    /// Returns a list of (file, version) tuples for files in the given target.
    pub async fn list_files_for_target(&self, target: impl AsRef<str>) -> Result<Vec<FileResult>> {
        let rows: Vec<FullJoinRow> = sqlx::query_as(include_str!("../queries/list_files_for_target.sql"))
            .bind(target.as_ref())
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// List all file paths for a specific target.
    ///
    /// This is more efficient than [`list_files_for_target`](Self::list_files_for_target)
    /// when you only need paths (e.g., for comparing against storage backend listing).
    pub async fn list_all_paths_for_target(&self, target: impl AsRef<str>) -> Result<Vec<String>> {
        let paths: Vec<String> = sqlx::query_scalar(include_str!("../queries/list_all_paths_for_target.sql"))
            .bind(target.as_ref())
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        Ok(paths)
    }

    /// List recently extracted files with their versions, ordered by extraction time.
    ///
    /// Useful for showing a picker of recent works.
    // TODO: Make a distinction between most recent discovered files and most recent imported files.
    pub async fn list_recent_files(&self, limit: usize) -> Result<Vec<FileResult>> {
        let limit = i64::try_from(limit).or_raise(|| ErrorKind::InvalidData("limit"))?;
        let rows: Vec<FullJoinRow> = sqlx::query_as(include_str!("../queries/list_recent_files.sql"))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        rows.into_iter().map(|r| r.try_into()).collect::<Result<Vec<_>>>()
    }

    /// List all distinct work IDs in the database.
    ///
    /// Useful for iterating over all works in the library.
    pub async fn list_all_work_ids(&self) -> Result<Vec<u64>> {
        let ids: Vec<i64> = sqlx::query_scalar(include_str!("../queries/list_all_work_ids.sql"))
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        let ids = ids
            .into_iter()
            .map(|id| u64::try_from(id).or_raise(|| ErrorKind::InvalidData("work id")))
            .collect::<Result<Vec<u64>>>()?;
        Ok(ids)
    }

    /// List all distinct work IDs that have files in the given target.
    pub async fn list_all_work_ids_for_target(&self, target: impl AsRef<str>) -> Result<Vec<u64>> {
        let ids: Vec<i64> = sqlx::query_scalar(include_str!("../queries/list_all_work_ids_for_target.sql"))
            .bind(target.as_ref())
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        let ids = ids
            .into_iter()
            .map(|id| u64::try_from(id).or_raise(|| ErrorKind::InvalidData("work id")))
            .collect::<Result<Vec<u64>>>()?;
        Ok(ids)
    }

    /* ============== *\
    |  Update Methods  |
    \* ============== */

    /// Update a file's path in the database (move/rename).
    ///
    /// Used during organize operations when files are moved to match the
    /// configured path template.
    ///
    /// Returns `true` if a record was updated, `false` if `old_path` was not found.
    pub async fn update_target_path(
        &self,
        target: impl AsRef<str>,
        old_path: impl AsRef<Path>,
        new_path: impl AsRef<Path>,
    ) -> Result<bool> {
        if self.dry_run {
            return Ok(true);
        }
        let result = sqlx::query(include_str!("../queries/update_target_path.sql"))
            .bind(Self::sqlx_hates_paths(new_path)?)
            .bind(target.as_ref())
            .bind(Self::sqlx_hates_paths(old_path)?)
            .execute(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        Ok(result.rows_affected() > 0)
    }

    /* ================ *\
    |  Existence Method  |
    \* ================ */

    /// Check if a file exists and whether its hash matches.
    ///
    /// This is the primary method for determining if a file needs to be
    /// re-imported during a scan operation:
    ///
    /// | Result                                | Action                                     |
    /// |---------------------------------------|--------------------------------------------|
    /// | [`ExistenceResult::NotFound`]         | File is new, needs full import             |
    /// | [`ExistenceResult::ExactMatch`]       | File unchanged, skip import                |
    /// | [`ExistenceResult::HashMismatch`]     | File changed, needs re-import              |
    /// | [`ExistenceResult::LocatedElsewhere`] | File is new, import but may re-use version |
    pub async fn exists(
        &self,
        target: impl AsRef<str>,
        path: impl AsRef<Path>,
        file_hash: impl AsRef<str>,
    ) -> Result<ExistenceResult> {
        if let Some((file, version)) = self.get_by_target_path(target, path).await? {
            return Ok(match file.file_hash == file_hash.as_ref() {
                true => ExistenceResult::ExactMatch(file, version),
                false => ExistenceResult::HashMismatch(file, version),
            });
        }
        Ok(match self.get_by_file_hash_across_targets(file_hash.as_ref()).await?.into_iter().next() {
            Some((file, version)) => ExistenceResult::LocatedElsewhere(file, version),
            _ => ExistenceResult::NotFound,
        })
    }

    /// Check if a file record exists at the given target and path, without
    /// fetching the full row.
    pub async fn target_path_exists(&self, target: impl AsRef<str>, path: impl AsRef<Path>) -> Result<bool> {
        let row: (i64,) = sqlx::query_as(include_str!("../queries/target_path_exists.sql"))
            .bind(target.as_ref())
            .bind(Self::sqlx_hates_paths(path)?)
            .fetch_one(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        Ok(row.0 > 0)
    }

    /// Check if a file with the given compressed file hash exists in any target.
    ///
    /// Useful for detecting if an identical compressed file exists elsewhere
    /// in the library or other targets.
    pub async fn file_hash_exists(&self, file_hash: impl AsRef<str>) -> Result<bool> {
        let row: (i64,) = sqlx::query_as(include_str!("../queries/file_hash_exists.sql"))
            .bind(file_hash.as_ref())
            .fetch_one(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        Ok(row.0 > 0)
    }

    /// Check if a version with the given decompressed content hash (BLAKE3 of
    /// the HTML) exists in the database.
    pub async fn content_hash_exists(&self, content_hash: impl AsRef<str>) -> Result<bool> {
        let row: (i64,) = sqlx::query_as(include_str!("../queries/content_hash_exists.sql"))
            .bind(content_hash.as_ref())
            .fetch_one(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        Ok(row.0 > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, File, Version};
    use rawr_compress::Compression;
    use rawr_extract::models::{Chapters, Language, Metadata, Rating};
    use rawr_storage::file::FileMeta;
    use time::{Date, UtcDateTime};

    const DEFAULT_TARGET: &str = "local";

    fn make_test_version(work_id: u64, content_hash: &str) -> Version {
        Version {
            hash: content_hash.to_string(),
            length: 1000,
            crc32: 12_345_678,
            metadata: Metadata {
                work_id,
                title: "Test Work".to_string(),
                authors: vec![],
                fandoms: vec![],
                rating: Some(Rating::GeneralAudiences),
                warnings: vec![],
                tags: vec![],
                summary: Some("A test work".to_string()),
                language: Language {
                    name: "English".to_string(),
                    iso_code: Some("en".to_string()),
                },
                chapters: Chapters { written: 1, total: Some(1) },
                words: 1000,
                published: Date::from_calendar_date(2024, time::Month::January, 1).unwrap(),
                last_modified: Date::from_calendar_date(2024, time::Month::January, 1).unwrap(),
                series: vec![],
            },
            extracted_at: UtcDateTime::now(),
        }
    }

    fn make_test_file(path: &str, content_hash: &str) -> File {
        FileMeta::new(DEFAULT_TARGET, path, Compression::Bzip2, 123, UtcDateTime::now())
            .with_file_hash("file_hash_123")
            .with_content_hash(content_hash)
    }

    async fn make_repository() -> Repository {
        let db = Database::connect_in_memory().await.unwrap();
        // ... database should drop at the end of this function, but
        // repository keeps working? What in the Mara Bos magic is this?
        Repository::from(&db)
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let repo = make_repository().await;
        let version = make_test_version(12345, "content_abc");
        let file = make_test_file("fandoms/work.html.bz2", "content_abc");
        repo.upsert(&file, &version).await.unwrap();
        let retrieved = repo.get_by_target_path(DEFAULT_TARGET, "fandoms/work.html.bz2").await.unwrap();
        assert!(retrieved.is_some());
        let (file, _version) = retrieved.unwrap();
        assert_eq!(file.content_hash, "content_abc");
    }
}
