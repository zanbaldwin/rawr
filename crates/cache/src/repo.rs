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

type FileRecord = FileInfo<Processed>;
type ModelPair = (FileRecord, Version);

fn group_files_by_version(rows: Vec<ModelPair>) -> Vec<(Version, Vec<FileRecord>)> {
    let mut map = HashMap::new();
    for (file, version) in rows {
        // TODO: Weep at the allocations. Burn this quick and dirty hack to the ground.
        let entry = map.entry(version.hash.clone()).or_insert_with(|| (version, Vec::new()));
        entry.1.push(file);
    }
    map.into_values().collect()
}

// fn group_files_by_version(rows: Vec<(Option<FileRecord>, Version)>) -> Vec<(Version, Vec<FileRecord>)> {
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
    pub async fn upsert(&self, file: &FileRecord, version: &Version) -> Result<()> {
        if file.content_hash != version.hash {
            exn::bail!(ErrorKind::Constraint);
        }
        if self.dry_run {
            return Ok(());
        }
        let version_row = VersionRow::try_from(version)?;
        let file_row = FileRow::try_from(file)?;
        let mut tx = self.pool.begin().await.or_raise(|| ErrorKind::Database)?;
        sqlx::query!(
            r#"
            INSERT INTO versions (
                content_hash,       content_crc32,  work_id,        content_size,
                title,              authors,        fandoms,        series,
                chapters_written,   chapters_total, words,          summary,
                rating,             warnings,       lang,           published_on,
                last_modified,      tags,           extracted_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (content_hash) DO NOTHING;
            "#,
            version_row.content_hash,
            version.crc32,
            version_row.work_id,
            version_row.content_size,
            version_row.title,
            version_row.authors,
            version_row.fandoms,
            version_row.series,
            version_row.chapters_written,
            version_row.chapters_total,
            version_row.words,
            version_row.summary,
            version_row.rating,
            version_row.warnings,
            version_row.lang,
            version_row.published_on,
            version_row.last_modified,
            version_row.tags,
            version_row.extracted_at,
        )
        .execute(&mut *tx)
        .await
        .or_raise(|| ErrorKind::Database)?;
        sqlx::query!(
            r#"
            INSERT INTO files (target, path, compression, file_size, file_hash, content_hash, discovered_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (target, path) DO UPDATE SET
                compression = excluded.compression,
                file_size = excluded.file_size,
                file_hash = excluded.file_hash,
                content_hash = excluded.content_hash
            "#,
            file_row.target,
            file_row.path,
            file_row.compression,
            file_row.file_size,
            file_row.file_hash,
            file_row.content_hash,
            file_row.discovered_at,
        )
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
    pub async fn get_by_path(&self, target: impl AsRef<str>, path: impl AsRef<Path>) -> Result<Option<ModelPair>> {
        let target = target.as_ref();
        let path = Self::sqlx_hates_paths(path)?;
        let row = sqlx::query_as!(
            JoinRow,
            r#"
            SELECT
                f.target,
                f.path,
                f.compression,
                f.file_size,
                f.file_hash,
                f.discovered_at,
                v.content_hash,
                v.content_crc32,
                v.work_id,
                v.content_size,
                v.title,
                v.authors,
                v.fandoms,
                v.series,
                v.chapters_written,
                v.chapters_total,
                v.words,
                v.summary,
                v.rating,
                v.warnings,
                v.lang,
                v.published_on,
                v.last_modified,
                v.tags,
                v.extracted_at
            FROM files f
            JOIN versions v ON f.content_hash = v.content_hash
            WHERE f.target = ? AND f.path = ?
            "#,
            target,
            path,
        )
        .fetch_optional(&self.pool)
        .await
        .or_raise(|| ErrorKind::Database)?;
        row.map(|r| r.try_into()).transpose()
    }

    /// Get a file and its version by the hash of the compressed file.
    ///
    /// The file is a BLAKE3 hash of the file as stored on disk (compressed).
    /// This is useful for detecting if a file's content has changed.
    ///
    /// > **Note:** Multiple files could theoretically have the same file hash
    /// > if they are exact copies (possibly in different targets).
    pub async fn get_by_file_hash(&self, file_hash: impl AsRef<str>) -> Result<Vec<ModelPair>> {
        let file_hash = file_hash.as_ref();
        let rows = sqlx::query_as!(
            JoinRow,
            r#"
            SELECT
                f.target,
                f.path,
                f.compression,
                f.file_size,
                f.file_hash,
                f.discovered_at,
                v.content_hash,
                v.content_crc32,
                v.work_id,
                v.content_size,
                v.title,
                v.authors,
                v.fandoms,
                v.series,
                v.chapters_written,
                v.chapters_total,
                v.words,
                v.summary,
                v.rating,
                v.warnings,
                v.lang,
                v.published_on,
                v.last_modified,
                v.tags,
                v.extracted_at
            FROM files f
            JOIN versions v ON f.content_hash = v.content_hash
            WHERE f.file_hash = ?
            "#,
            file_hash,
        )
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
    pub async fn get_by_content_hash(
        &self,
        content_hash: impl AsRef<str>,
    ) -> Result<Option<(Version, Vec<FileRecord>)>> {
        let content_hash = content_hash.as_ref();
        let rows = sqlx::query_as!(
            JoinRow,
            r#"
            SELECT
                f.target,
                f.path,
                f.compression,
                f.file_size,
                f.file_hash,
                f.discovered_at,
                v.content_hash,
                v.content_crc32,
                v.work_id,
                v.content_size,
                v.title,
                v.authors,
                v.fandoms,
                v.series,
                v.chapters_written,
                v.chapters_total,
                v.words,
                v.summary,
                v.rating,
                v.warnings,
                v.lang,
                v.published_on,
                v.last_modified,
                v.tags,
                v.extracted_at
            FROM versions v
            LEFT JOIN files f ON f.content_hash = v.content_hash
            WHERE v.content_hash = ?
            "#,
            content_hash,
        )
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
    pub async fn get_by_work_id(&self, work_id: u64) -> Result<Vec<(Version, Vec<FileRecord>)>> {
        let work_id = i64::try_from(work_id).or_raise(|| ErrorKind::InvalidData("work id"))?;
        let rows = sqlx::query_as!(
            JoinRow,
            r#"
            SELECT
                f.target,
                f.path,
                f.compression,
                f.file_size,
                f.file_hash,
                f.discovered_at,
                v.content_hash,
                v.content_crc32,
                v.work_id,
                v.content_size,
                v.title,
                v.authors,
                v.fandoms,
                v.series,
                v.chapters_written,
                v.chapters_total,
                v.words,
                v.summary,
                v.rating,
                v.warnings,
                v.lang,
                v.published_on,
                v.last_modified,
                v.tags,
                v.extracted_at
            FROM versions v
            JOIN files f ON f.content_hash = v.content_hash
            WHERE v.work_id = ?
            "#,
            work_id,
        )
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
    pub async fn get_best_for_work_id(&self, work_id: u64) -> Result<Option<(Version, Vec<FileRecord>)>> {
        // TODO: This is NOT more efficient, but it IS easier to implement and
        //       that's all we care about right now.
        let results = self.get_by_work_id(work_id).await?;
        Ok(results.into_iter().next())
    }

    // =========================================================================
    // Listing
    // =========================================================================

    pub async fn list_scanned_targets(&self) -> Result<Vec<String>> {
        todo!()
    }

    /// List all versions and their associated files across all targets.
    ///
    /// Returns a list of (version, files) tuples. Each version appears once
    /// with all files that reference it (potentially from multiple targets).
    pub async fn list_versions_for_target(&self, target: impl AsRef<str>) -> Result<Vec<(Version, Vec<FileRecord>)>> {
        let target = target.as_ref();
        let rows = sqlx::query_as!(
            JoinRow,
            r#"
            SELECT
                f.target,
                f.path,
                f.compression,
                f.file_size,
                f.file_hash,
                f.discovered_at,
                v.content_hash,
                v.content_crc32,
                v.work_id,
                v.content_size,
                v.title,
                v.authors,
                v.fandoms,
                v.series,
                v.chapters_written,
                v.chapters_total,
                v.words,
                v.summary,
                v.rating,
                v.warnings,
                v.lang,
                v.published_on,
                v.last_modified,
                v.tags,
                v.extracted_at
            FROM versions v
            JOIN files f ON f.content_hash = v.content_hash
            WHERE f.target = ?
            "#,
            target,
        )
        .fetch_all(&self.pool)
        .await
        .or_raise(|| ErrorKind::Database)?;
        let pairs = rows.into_iter().map(|r| r.try_into()).collect::<Result<Vec<_>>>()?;
        Ok(group_files_by_version(pairs))
    }

    /// List all files for a specific target.
    ///
    /// Returns a list of (file, version) tuples for files in the given target.
    pub async fn list_files_for_target(&self, target: impl AsRef<str>) -> Result<Vec<ModelPair>> {
        let target = target.as_ref();
        let rows = sqlx::query_as!(
            JoinRow,
            r#"
            SELECT
                f.target,
                f.path,
                f.compression,
                f.file_size,
                f.file_hash,
                f.discovered_at,
                v.content_hash,
                v.content_crc32,
                v.work_id,
                v.content_size,
                v.title,
                v.authors,
                v.fandoms,
                v.series,
                v.chapters_written,
                v.chapters_total,
                v.words,
                v.summary,
                v.rating,
                v.warnings,
                v.lang,
                v.published_on,
                v.last_modified,
                v.tags,
                v.extracted_at
            FROM files f
            JOIN versions v ON f.content_hash = v.content_hash
            WHERE f.target = ?
            ORDER BY f.path
            "#,
            target,
        )
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
        let target = target.as_ref();
        let paths = sqlx::query_scalar!(
            r#"
            SELECT path FROM files WHERE target = ? ORDER BY path
            "#,
            target,
        )
        .fetch_all(&self.pool)
        .await
        .or_raise(|| ErrorKind::Database)?;
        Ok(paths)
    }

    /// List recently extracted files with their versions, ordered by extraction time.
    ///
    /// Useful for showing a picker of recent works. Returns (FileRecord, Version) tuples
    /// sorted by `extracted_at` descending (most recent first).
    pub async fn list_recent_files(&self, limit: usize) -> Result<Vec<ModelPair>> {
        let limit = i64::try_from(limit).or_raise(|| ErrorKind::InvalidData("limit"))?;
        let rows = sqlx::query_as!(
            JoinRow,
            r#"
            SELECT
                f.target,
                f.path,
                f.compression,
                f.file_size,
                f.file_hash,
                f.discovered_at,
                v.content_hash,
                v.content_crc32,
                v.work_id,
                v.content_size,
                v.title,
                v.authors,
                v.fandoms,
                v.series,
                v.chapters_written,
                v.chapters_total,
                v.words,
                v.summary,
                v.rating,
                v.warnings,
                v.lang,
                v.published_on,
                v.last_modified,
                v.tags,
                v.extracted_at
            FROM files f
            JOIN versions v ON f.content_hash = v.content_hash
            ORDER BY f.discovered_at DESC
            LIMIT ?
            "#,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .or_raise(|| ErrorKind::Database)?;
        rows.into_iter().map(|r| r.try_into()).collect::<Result<Vec<_>>>()
    }

    /// List all distinct work IDs in the database.
    ///
    /// Useful for iterating over all works in the library.
    pub async fn list_all_work_ids(&self) -> Result<Vec<u64>> {
        let ids = sqlx::query_scalar!(
            r#"
            SELECT DISTINCT work_id
            FROM versions
            ORDER BY work_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .or_raise(|| ErrorKind::Database)?;
        let ids = ids
            .into_iter()
            .map(|id| u64::try_from(id).or_raise(|| ErrorKind::InvalidData("work id")))
            .collect::<Result<Vec<u64>>>()?;
        Ok(ids)
    }
}
