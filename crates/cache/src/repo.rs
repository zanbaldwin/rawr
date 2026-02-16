//! Combined repository for File and Version structs.
//!
//! They're tightly coupled: can't have a File without a Version, and there's
//! no point keeping a version if there's no physical file to extract it from
//! (unless for historical record keeping).

use crate::error::{ErrorKind, Result};
use crate::models::{FileRow, VersionRow};
use crate::{Database, File, Version};
use exn::ResultExt;
use sqlx::SqlitePool;

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
}
