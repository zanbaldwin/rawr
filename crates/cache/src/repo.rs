//! Combined repository for FileRecord and Version entities.
//!
//! They're tightly coupled: can't have a FileRecord without a Version, and
//! there's no point keeping a version if there's no physical file to extract it
//! from (unless for historical record keeping).

use crate::Database;
use crate::error::{ErrorKind, Result};
use crate::models::{FileRow, JoinRow, VersionRow};
use exn::{OptionExt, ResultExt};
use rawr_extract::models::Version;
use rawr_storage::file::{FileInfo, Processed};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::Path;

type File = FileInfo<Processed>;

type RowPair = (FileRow, VersionRow);
type OrphanRowPair = (Option<FileRow>, VersionRow);

type OrphanModelPair = (Option<File>, Version);

type FileResult = (File, Version);
type VersionResult = (Version, Vec<File>);

/// Result of checking whether a file exists in the cache.
#[derive(Debug, Eq, PartialEq)]
pub enum ExistenceResult {
    /// No file record exists at the given path.
    NotFound,
    /// A file record exists at the given path and the file hash matches.
    ExactMatch(File, Version),
    /// A file record exists at the given path, but the current file hash does
    /// not match what's on record. but the file hash differs.
    ///
    /// The file could be corrupt, it could have been replaced, it could have
    /// been recompressed to another format. Whatever happened, the current file
    /// record is stale and the file needs to be re-imported to update the cache.
    HashMismatch(File, Version),
    /// The specified file is not recorded in the cache database, but a file
    /// with the same hash is known about at a different location.
    LocatedElsewhere(File, Version),
}

fn group_files_by_version(rows: Vec<FileResult>) -> Vec<VersionResult> {
    let mut map = HashMap::new();
    for (file, version) in rows {
        // TODO: Weep at the allocations. Burn this quick and dirty hack to the ground.
        let entry = map.entry(version.hash.clone()).or_insert_with(|| (version, Vec::new()));
        entry.1.push(file);
    }
    map.into_values().collect()
}

// fn group_optional_files_by_version(rows: Vec<OrphanModelPair>) -> Vec<VersionResult> {
//     let mut map: HashMap<String, (Version, Vec<FileRecord>)> = HashMap::new();
//     for (file, version) in rows {
//         let entry = map.entry(version.hash.clone()).or_insert_with(|| (version, Vec::new()));
//         if let Some(file) = file {
//             entry.1.push(file);
//         }
//     }
//     map.into_values().collect()
// }

/// Repository for managing File and Version entries in the cache database.
///
/// This repository treats files and versions as a unit. Files track physical
/// locations in the library (paths), while versions track unique content
/// (identified by BLAKE3 hash of decompressed HTML) and its extracted metadata.
///
/// # Relationships
///
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

    // =========================================================================
    // Insert
    // =========================================================================

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
            .bind(version.crc32)
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

    // =========================================================================
    // Get/Fetch
    // =========================================================================

    /// Get a file and its version by target and path.
    ///
    /// The path is relative to the target root (e.g., `"Fandom/Author - Title.html.bz2"`).
    pub async fn get_by_target_path(
        &self,
        target: impl AsRef<str>,
        path: impl AsRef<Path>,
    ) -> Result<Option<FileResult>> {
        let row: Option<JoinRow> = sqlx::query_as(include_str!("../queries/get_by_target_path.sql"))
            .bind(target.as_ref())
            .bind(Self::sqlx_hates_paths(path)?)
            .fetch_optional(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        row.map(|r| r.try_into()).transpose()
    }

    /// Get a file and its version by path, across all targets.
    ///
    /// The path is relative to the target root (e.g., `"Fandom/Author - Title.html.bz2"`).
    pub async fn get_by_path_across_targets(&self, path: impl AsRef<Path>) -> Result<Vec<FileResult>> {
        let row: Vec<JoinRow> = sqlx::query_as(include_str!("../queries/get_by_path_across_targets.sql"))
            .bind(Self::sqlx_hates_paths(path)?)
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        row.into_iter().map(|r| r.try_into()).collect::<Result<Vec<_>>>()
    }

    /// Get a file and its version by the hash of the compressed file.
    ///
    /// The file is a BLAKE3 hash of the file as stored on disk (compressed).
    /// This is useful for detecting if a file's content has changed.
    ///
    /// > **Note:** Multiple files could theoretically have the same file hash
    /// > if they are exact copies (possibly in different targets).
    pub async fn get_by_file_hash(&self, file_hash: impl AsRef<str>) -> Result<Vec<FileResult>> {
        let file_hash = file_hash.as_ref();
        let rows: Vec<JoinRow> = sqlx::query_as(include_str!("../queries/get_by_file_hash.sql"))
            .bind(file_hash)
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
        let rows: Vec<OrphanJoinRow> = sqlx::query_as(include_str!("../queries/get_by_content_hash.sql"))
            .bind(content_hash)
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let pairs = rows.into_iter().map(|r| r.try_into()).collect::<Result<Vec<_>>>()?;
        Ok(group_files_by_version(pairs).into_iter().next())
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
        let rows: Vec<OrphanJoinRow> = sqlx::query_as(include_str!("../queries/get_by_work_id.sql"))
            .bind(work_id)
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        let pairs = rows.into_iter().map(|r| r.try_into()).collect::<Result<Vec<_>>>()?;
        let mut map = group_files_by_version(pairs);
        map.sort_by(|(a, _), (b, _)| b.cmp(a));
        Ok(map)
    }

    /// Get the best version for a work ID and all files referencing it.
    ///
    /// "Best" is determined by the version comparison algorithm in `rawr-extract`,
    /// which considers factors like last modified date, chapter count, and
    /// file size.
    ///
    /// > **Note::** This is equivalent to `get_by_work_id(work_id)?.first()`
    /// > but more efficient.
    pub async fn get_best_for_work_id(&self, work_id: u64) -> Result<Option<VersionResult>> {
        // TODO: This is NOT more efficient, but it IS easier to implement and
        //       that's all we care about right now.
        let results = self.get_by_work_id(work_id).await?;
        Ok(results.into_iter().next())
    }

    // =========================================================================
    // Listing
    // =========================================================================

    pub async fn list_scanned_targets(&self) -> Result<Vec<String>> {
        let targets: Vec<String> = sqlx::query_scalar(include_str!("../queries/list_scanned_targets.sql"))
            .fetch_all(&self.pool)
            .await
            .or_raise(|| ErrorKind::Database)?;
        Ok(targets)
    }

    /// List all versions and their associated files across all targets.
    ///
    /// Returns a list of (version, files) tuples. Each version appears once
    /// with all files that reference it (potentially from multiple targets).
    pub async fn list_versions_for_target(&self, target: impl AsRef<str>) -> Result<Vec<VersionResult>> {
        let rows: Vec<JoinRow> = sqlx::query_as(include_str!("../queries/list_versions_for_target.sql"))
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
        let rows: Vec<JoinRow> = sqlx::query_as(include_str!("../queries/list_files_for_target.sql"))
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
        let rows: Vec<JoinRow> = sqlx::query_as(include_str!("../queries/list_recent_files.sql"))
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

    /// List all distinct work IDs on a target.
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

    // // =========================================================================
    // // Update
    // // =========================================================================

    // /// Update a file's path in the database.
    // ///
    // /// Used during organize operations when files are moved to match the
    // /// configured path template.
    // ///
    // /// Returns `true` if a record was updated, `false` if `old_path` was not found.
    // pub async fn update_path(
    //     &self,
    //     target: impl AsRef<str>,
    //     old_path: impl AsRef<str>,
    //     new_path: impl AsRef<str>,
    // ) -> Result<bool, CacheError> {
    //     if self.dry_run {
    //         return Ok(true);
    //     }
    //     let result = sqlx::query(
    //         r#"
    //         UPDATE files SET path = ?, last_verified_at = ? WHERE target = ? AND path = ?
    //         "#,
    //     )
    //     .bind(new_path.as_ref())
    //     .bind(time::OffsetDateTime::now_utc().unix_timestamp())
    //     .bind(target.as_ref())
    //     .bind(old_path.as_ref())
    //     .execute(&self.pool)
    //     .await;
    //     let result = match result {
    //         Ok(v) => Ok(v),
    //         Err(e) => {
    //             eprintln!("{:?}  {:?}", old_path.as_ref(), new_path.as_ref());
    //             Err(e)
    //         },
    //     }?;
    //     Ok(result.rows_affected() > 0)
    // }

    // /// Update the last_verified_at timestamp for a file.
    // pub async fn update_last_verified(
    //     &self,
    //     target: impl AsRef<str>,
    //     path: impl AsRef<str>,
    // ) -> Result<bool, CacheError> {
    //     if self.dry_run {
    //         return Ok(true);
    //     }
    //     let result = sqlx::query(
    //         r#"
    //         UPDATE files SET last_verified_at = ? WHERE target = ? AND path = ?
    //         "#,
    //     )
    //     .bind(time::OffsetDateTime::now_utc().unix_timestamp())
    //     .bind(target.as_ref())
    //     .bind(path.as_ref())
    //     .execute(&self.pool)
    //     .await?;
    //     Ok(result.rows_affected() > 0)
    // }

    // // =========================================================================
    // // Existence
    // // =========================================================================

    // /// Check if a file exists and whether its hash matches.
    // ///
    // /// This is the primary method for determining if a file needs to be
    // /// re-imported during a scan operation:
    // ///
    // /// | Result             | Action                                     |
    // /// |--------------------|--------------------------------------------|
    // /// | `NotFound`         | File is new, needs full import             |
    // /// | `ExactMatch`       | File unchanged, skip import                |
    // /// | `HashMismatch`     | File changed, needs re-import              |
    // /// | `LocatedElsewhere` | File is new, import but may re-use version |
    // pub async fn exists(
    //     &self,
    //     target: impl AsRef<str>,
    //     path: impl AsRef<str>,
    //     file_hash: impl AsRef<str>,
    // ) -> Result<ExistenceResult, CacheError> {
    //     if let Some((file, version)) = self.get_by_path(target.as_ref(), path.as_ref()).await? {
    //         return Ok(match file.file_hash == file_hash.as_ref() {
    //             true => ExistenceResult::ExactMatch(file, version),
    //             false => ExistenceResult::HashMismatch(file, version),
    //         });
    //     }
    //     Ok(match self.get_by_file_hash(file_hash.as_ref()).await?.into_iter().next() {
    //         Some((file, version)) => ExistenceResult::LocatedElsewhere(file, version),
    //         _ => ExistenceResult::NotFound,
    //     })
    // }

    // /// Check if a file record exists at the given target and path.
    // pub async fn path_exists(&self, target: impl AsRef<str>, path: impl AsRef<str>) -> Result<bool, CacheError> {
    //     let row: (i64,) = sqlx::query_as(
    //         r#"
    //         SELECT COUNT(*) FROM files WHERE target = ? AND path = ?
    //         "#,
    //     )
    //     .bind(target.as_ref())
    //     .bind(path.as_ref())
    //     .fetch_one(&self.pool)
    //     .await?;
    //     Ok(row.0 > 0)
    // }

    // /// Check if a file with the given compressed file hash exists in any target.
    // ///
    // /// Useful for detecting if an identical compressed file exists elsewhere
    // /// in the library or other targets.
    // pub async fn file_hash_exists(&self, file_hash: impl AsRef<str>) -> Result<bool, CacheError> {
    //     let row: (i64,) = sqlx::query_as(
    //         r#"
    //         SELECT COUNT(*) FROM files WHERE file_hash = ?
    //         "#,
    //     )
    //     .bind(file_hash.as_ref())
    //     .fetch_one(&self.pool)
    //     .await?;
    //     Ok(row.0 > 0)
    // }

    // // =========================================================================
    // // Counts
    // // =========================================================================

    // /// Count the total number of file records in the database.
    // pub async fn count_files(&self) -> Result<u64, CacheError> {
    //     let row: (i64,) = sqlx::query_as(
    //         r#"
    //         SELECT COUNT(*) FROM files
    //         "#,
    //     )
    //     .fetch_one(&self.pool)
    //     .await?;
    //     Ok(row.0 as u64)
    // }

    // /// Count the total number of versions in the database.
    // pub async fn count_versions(&self) -> Result<u64, CacheError> {
    //     let row: (i64,) = sqlx::query_as(
    //         r#"
    //         SELECT COUNT(*) FROM versions
    //         "#,
    //     )
    //     .fetch_one(&self.pool)
    //     .await?;
    //     Ok(row.0 as u64)
    // }

    // /// Count the number of distinct works in the database.
    // pub async fn count_works(&self) -> Result<u64, CacheError> {
    //     let row: (i64,) = sqlx::query_as(
    //         r#"
    //         SELECT COUNT(DISTINCT work_id) FROM versions
    //         "#,
    //     )
    //     .fetch_one(&self.pool)
    //     .await?;
    //     Ok(row.0 as u64)
    // }

    // // =========================================================================
    // // Duplicates
    // // =========================================================================

    // /// Find content hashes that have multiple files pointing to them.
    // ///
    // /// These are duplicate files (same content at different paths). Returns
    // /// a list of (content_hash, file_count) tuples, sorted by count descending.
    // ///
    // /// Use [`get_by_content_hash`](Self::get_by_content_hash) to retrieve
    // /// the actual files for cleanup.
    // pub async fn find_duplicate_content(&self) -> Result<Vec<(String, u64)>, CacheError> {
    //     let rows: Vec<(String, i64)> = sqlx::query_as(
    //         r#"
    //         SELECT content_hash, COUNT(*) as count
    //         FROM files
    //         GROUP BY content_hash
    //         HAVING count > 1
    //         ORDER BY count DESC
    //         "#,
    //     )
    //     .fetch_all(&self.pool)
    //     .await?;
    //     Ok(rows.into_iter().map(|(hash, count)| (hash, count as u64)).collect())
    // }

    // /// Find works that have multiple versions.
    // ///
    // /// These may be intentional (tracking history) or candidates for cleanup.
    // /// Returns a list of (work_id, version_count) tuples, sorted by count descending.
    // ///
    // /// Use [`get_by_work_id`](Self::get_by_work_id) to retrieve the versions
    // /// and determine which to keep.
    // pub async fn find_works_with_multiple_versions(&self) -> Result<Vec<(u64, u64)>, CacheError> {
    //     let rows: Vec<(i64, i64)> = sqlx::query_as(
    //         r#"
    //         SELECT work_id, COUNT(*) as count
    //         FROM versions
    //         GROUP BY work_id
    //         HAVING count > 1
    //         ORDER BY count DESC
    //         "#,
    //     )
    //     .fetch_all(&self.pool)
    //     .await?;
    //     Ok(rows.into_iter().map(|(id, count)| (id as u64, count as u64)).collect())
    // }

    // // =========================================================================
    // // Delete
    // // =========================================================================

    // /// Delete a file record by its target and path.
    // ///
    // /// Only deletes the file record, not the version. If this was the last
    // /// file referencing a version, the version becomes orphaned. Call
    // /// [`delete_orphaned_versions`](Self::delete_orphaned_versions) to clean
    // /// up orphans if `retain_deleted_versions` is not enabled.
    // ///
    // /// Returns `true` if a record was deleted, `false` if the path was not found.
    // pub async fn delete_by_path(&self, target: impl AsRef<str>, path: impl AsRef<str>) -> Result<bool, CacheError> {
    //     if self.dry_run {
    //         return Ok(true);
    //     }
    //     let result = sqlx::query(
    //         r#"
    //         DELETE FROM files WHERE target = ? AND path = ?
    //         "#,
    //     )
    //     .bind(target.as_ref())
    //     .bind(path.as_ref())
    //     .execute(&self.pool)
    //     .await?;
    //     // Calling self.delete_orphaned_versions() depending on the value of
    //     // `self.config.library.retain_deleted_versions` is the
    //     // responsiblity of the callee (orchestrator in app binary).
    //     Ok(result.rows_affected() > 0)
    // }

    // /// Delete file records for paths that no longer exist in a target.
    // ///
    // /// Given a list of paths that currently exist in a target, deletes all
    // /// file records for that target whose paths are not in the list.
    // ///
    // /// Returns the number of records deleted.
    // pub async fn delete_missing_from_target(
    //     &self,
    //     target: impl AsRef<str>,
    //     existing_paths: &[String],
    // ) -> Result<u64, CacheError> {
    //     if self.dry_run {
    //         // In dry-run mode, return the count of what would be deleted
    //         let cached_paths = self.list_paths_for_target(target.as_ref()).await?;
    //         let count = cached_paths.iter().filter(|p| !existing_paths.contains(p)).count();
    //         return Ok(count as u64);
    //     }

    //     // For efficiency with large lists, we use a transaction and batch delete
    //     let cached_paths = self.list_paths_for_target(target.as_ref()).await?;
    //     let to_delete: Vec<_> = cached_paths.iter().filter(|p| !existing_paths.contains(p)).collect();

    //     if to_delete.is_empty() {
    //         return Ok(0);
    //     }

    //     let mut deleted = 0u64;
    //     for path in to_delete {
    //         if self.delete_by_path(target.as_ref(), path).await? {
    //             deleted += 1;
    //         }
    //     }
    //     Ok(deleted)
    // }

    // /// Delete all file records with the given compressed file hash.
    // ///
    // /// Returns `true` if any records were deleted.
    // pub async fn delete_by_file_hash(&self, file_hash: impl AsRef<str>) -> Result<bool, CacheError> {
    //     if self.dry_run {
    //         return Ok(true);
    //     }
    //     let result = sqlx::query(
    //         r#"
    //         DELETE FROM files WHERE file_hash = ?
    //         "#,
    //     )
    //     .bind(file_hash.as_ref())
    //     .execute(&self.pool)
    //     .await?;
    //     Ok(result.rows_affected() > 0)
    // }

    // /// Delete a version and all files referencing it.
    // ///
    // /// Due to CASCADE, deleting a version automatically deletes all file
    // /// records that reference it.
    // ///
    // /// Returns `true` if the version was deleted, `false` if it was not found.
    // pub async fn delete_by_content_hash(&self, content_hash: impl AsRef<str>) -> Result<bool, CacheError> {
    //     if self.dry_run {
    //         return Ok(true);
    //     }
    //     let result = sqlx::query(
    //         r#"
    //         DELETE FROM versions WHERE content_hash = ?
    //         "#,
    //     )
    //     .bind(content_hash.as_ref())
    //     .execute(&self.pool)
    //     .await?;
    //     Ok(result.rows_affected() > 0)
    // }

    // /// Delete all versions and files for a given work ID.
    // ///
    // /// This removes the work entirely from the cache (versions and file
    // /// records). The actual files on disk are not affected.
    // ///
    // /// Returns `true` if any versions were deleted.
    // pub async fn delete_by_work_id(&self, work_id: u64) -> Result<bool, CacheError> {
    //     if self.dry_run {
    //         return Ok(true);
    //     }
    //     let result = sqlx::query(
    //         r#"
    //         DELETE FROM versions WHERE work_id = ?
    //         "#,
    //     )
    //     .bind(work_id as i64)
    //     .execute(&self.pool)
    //     .await?;
    //     Ok(result.rows_affected() > 0)
    // }

    // /// Delete all versions that have no files referencing them.
    // ///
    // /// Versions become orphaned when all their files are deleted (e.g., via
    // /// [`delete_by_path`](Self::delete_by_path)). This cleans them up.
    // ///
    // /// Whether to call this automatically is controlled by the
    // /// `retain_deleted_versions` configuration option in the app binary, and
    // /// is the responsibility of the repository callee.
    // ///
    // /// Returns the number of orphaned versions deleted.
    // pub async fn delete_orphaned_versions(&self) -> Result<u64, CacheError> {
    //     if self.dry_run {
    //         let row: (i64,) = sqlx::query_as(
    //             r#"
    //             SELECT COUNT(*)
    //             FROM versions
    //             WHERE content_hash NOT IN (SELECT content_hash FROM files)
    //             "#,
    //         )
    //         .fetch_one(&self.pool)
    //         .await?;
    //         return Ok(row.0 as u64);
    //     }
    //     let result = sqlx::query(
    //         r#"
    //         DELETE FROM versions
    //         WHERE content_hash NOT IN (SELECT content_hash FROM files)
    //         "#,
    //     )
    //     .execute(&self.pool)
    //     .await?;
    //     Ok(result.rows_affected())
    // }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::Database;
//     use rawr_compress::Compression;
//     use rawr_extract::Version;
//     use rawr_extract::models::{Chapters, Language, Metadata, Rating};
//     use time::{Date, OffsetDateTime};

//     fn make_test_version(work_id: u64, content_hash: &str) -> Version {
//         Version {
//             content_hash: content_hash.to_string(),
//             content_length: 1000,
//             content_crc32: "abcd1234".to_string(),
//             metadata: Metadata {
//                 work_id,
//                 title: "Test Work".to_string(),
//                 authors: vec![],
//                 fandoms: vec![],
//                 rating: Rating::GeneralAudiences,
//                 warnings: vec![],
//                 tags: vec![],
//                 summary: "A test work".to_string(),
//                 language: Language {
//                     name: "English".to_string(),
//                     iso_code: Some("en".to_string()),
//                 },
//                 chapters: Chapters { written: 1, total: Some(1) },
//                 words: 1000,
//                 published: Date::from_calendar_date(2024, time::Month::January, 1).unwrap(),
//                 last_modified: Date::from_calendar_date(2024, time::Month::January, 1).unwrap(),
//                 series: vec![],
//             },
//         }
//     }

//     fn make_test_file(path: &str, content_hash: &str) -> FileRecord {
//         FileRecord::new(
//             "local",
//             path,
//             Compression::Bzip2,
//             123,
//             "file_hash_123",
//             content_hash,
//             OffsetDateTime::now_utc(),
//         )
//     }

//     #[tokio::test]
//     async fn test_insert_and_get() {
//         let db = Database::connect_in_memory().await.unwrap();
//         let repo = FileVersionRepository::new(db.pool().clone(), false);
//         let version = make_test_version(12345, "content_abc");
//         let file = make_test_file("fandoms/work.html.bz2", "content_abc");
//         repo.insert(&file, &version).await.unwrap();
//         let retrieved = repo.get_by_path("local", "fandoms/work.html.bz2").await.unwrap();
//         assert!(retrieved.is_some());
//         let (file, _version) = retrieved.unwrap();
//         assert_eq!(file.content_hash, "content_abc");
//     }

//     #[tokio::test]
//     async fn test_get_by_content_hash() {
//         let db = Database::connect_in_memory().await.unwrap();
//         let repo = FileVersionRepository::new(db.pool().clone(), false);
//         let version = make_test_version(12345, "content_abc");
//         let file1 = make_test_file("file1.html.bz2", "content_abc");
//         repo.insert(&file1, &version).await.unwrap();
//         let file2 = make_test_file("file2.html.bz2", "content_abc");
//         repo.insert(&file2, &version).await.unwrap();
//         let (_version, files) = repo.get_by_content_hash("content_abc").await.unwrap().unwrap();
//         assert_eq!(2, files.len());
//         repo.delete_by_path("local", "file1.html.bz2").await.unwrap();
//         let (_version, files) = repo.get_by_content_hash("content_abc").await.unwrap().unwrap();
//         assert_eq!(1, files.len());
//         repo.delete_by_path("local", "file2.html.bz2").await.unwrap();
//         let (_version, files) = repo.get_by_content_hash("content_abc").await.unwrap().unwrap();
//         assert_eq!(0, files.len());
//     }

//     #[tokio::test]
//     async fn test_update_path() {
//         let db = Database::connect_in_memory().await.unwrap();
//         let repo = FileVersionRepository::new(db.pool().clone(), false);
//         let version = make_test_version(12345, "content_abc");
//         let file = make_test_file("old/path.html.bz2", "content_abc");
//         repo.insert(&file, &version).await.unwrap();
//         let updated = repo.update_path("local", "old/path.html.bz2", "new/path.html.bz2").await.unwrap();
//         assert!(updated);
//         assert!(repo.get_by_path("local", "old/path.html.bz2").await.unwrap().is_none());
//         assert!(repo.get_by_path("local", "new/path.html.bz2").await.unwrap().is_some());
//     }

//     #[tokio::test]
//     async fn test_cascade_delete() {
//         let db = Database::connect_in_memory().await.unwrap();
//         let repo = FileVersionRepository::new(db.pool().clone(), false);
//         let version = make_test_version(12345, "content_abc");
//         let file = make_test_file("fandoms/work.html.bz2", "content_abc");
//         repo.insert(&file, &version).await.unwrap();
//         // Delete version should cascade to file
//         repo.delete_by_content_hash("content_abc").await.unwrap();
//         let retrieved = repo.get_by_path("local", "fandoms/work.html.bz2").await.unwrap();
//         assert!(retrieved.is_none());
//     }

//     #[tokio::test]
//     async fn test_find_duplicate_content() {
//         let db = Database::connect_in_memory().await.unwrap();
//         let repo = FileVersionRepository::new(db.pool().clone(), false);
//         let version1 = make_test_version(111, "hash1");
//         let version2 = make_test_version(222, "hash2");
//         let file1 = make_test_file("path1.html.bz2", "hash1");
//         let file2 = make_test_file("path2.html.bz2", "hash1");
//         let file3 = make_test_file("path3.html.bz2", "hash2");
//         repo.insert(&file1, &version1).await.unwrap();
//         repo.insert(&file2, &version1).await.unwrap();
//         repo.insert(&file3, &version2).await.unwrap();
//         let dups = repo.find_duplicate_content().await.unwrap();
//         assert_eq!(dups.len(), 1);
//         assert_eq!(dups[0], ("hash1".to_string(), 2));
//     }
// }
