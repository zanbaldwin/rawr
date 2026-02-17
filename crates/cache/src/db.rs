//! Database connection and pool management.

use exn::ResultExt;
use sqlx::SqliteConnection;
use sqlx::pool::PoolConnectionMetadata;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteSynchronous};
use std::path::Path;
use tracing::instrument;

use crate::error::{ErrorKind, Result};

/// Embedded migrations that are run automatically on connect.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
// We want to make use of that async-goodness, so... 5-ish?
const MAX_CONNECTIONS: u32 = 5;

/// Database connection pool for the cache.
///
/// This is the main entry point for interacting with the cache database.
/// It manages the SQLite connection pool and provides access to repositories.
#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    async fn new(options: SqliteConnectOptions, max: Option<u32>) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            // This is IMPORTANT to apply the query-based PRAGMAs to EVERY
            // connection (set by max connections) instead of only the
            // first connection returned by the pool.
            .after_connect(|conn, meta| Box::pin(async move {
                Self::apply_pragmas(conn, meta).await
            }))
            .max_connections(max.unwrap_or(MAX_CONNECTIONS))
            .connect_with(options)
            .await
            .or_raise(|| ErrorKind::Database)?;
        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    /// Connect to the cache database at the given path.
    ///
    /// Creates the database file if it doesn't exist and runs migrations.
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let options = Self::base_options().filename(path).create_if_missing(true);
        Self::new(options, None).await
    }

    /// Connect to an in-memory database (useful for testing).
    ///
    /// Note:
    /// - In-memory databases are destroyed when the connection closes.
    /// - Do NOT apply `#[cfg(test)]` so that other crates can also use this in their tests.
    pub async fn connect_in_memory() -> Result<Self> {
        let options = Self::base_options().filename(":memory:");
        // In-memory database must either use the same cache `.shared_cache(true)`,
        // or be limited to one connection. Otherwise parallel connections will
        // see different databases that contain different data.
        Self::new(options, Some(1)).await
    }

    /// Base connection options shared between file and in-memory databases.
    fn base_options() -> SqliteConnectOptions {
        SqliteConnectOptions::new()
            // Enable WAL mode for better concurrent read performance
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            // Foreign key enforcement
            .foreign_keys(true)
            // PRAGMA synchronous = NORMAL (balance between safety and speed)
            .synchronous(SqliteSynchronous::Normal)
            // PRAGMA busy_timeout = 1500ms
            // An async library scan of thousands of files could result in
            // SQLITE_BUSY on too small timeouts with only one writer in
            // WAL-mode, even if most of the waiting is I/O-bound.
            .busy_timeout(std::time::Duration::from_millis(1500))
            // PRAGMA auto_vacuum = OFF (default, but explicit)
            // Make more efficient use of already used space, instead of
            // trying to optimize for size.
            .auto_vacuum(sqlx::sqlite::SqliteAutoVacuum::None)
    }

    /// Apply additional PRAGMA settings that aren't exposed via SqliteConnectOptions.
    async fn apply_pragmas(conn: &mut SqliteConnection, _meta: PoolConnectionMetadata) -> sqlx::Result<()> {
        sqlx::query(
            r#"
                PRAGMA locking_mode = NORMAL;
                PRAGMA wal_autocheckpoint = 800;
                PRAGMA cache_size = -8192;
                PRAGMA temp_store = MEMORY;
                PRAGMA mmap_size = 33554432;
                PRAGMA analysis_limit = 1000;
            "#,
        )
        .execute(conn)
        .await?;
        Ok(())
    }

    /// Run database migrations.
    ///
    /// This is called automatically by `connect` and `connect_in_memory`,
    /// but can be called manually if needed.
    #[instrument("performing database migrations")]
    async fn migrate(&self) -> Result<()> {
        MIGRATOR.run(&self.pool).await.or_raise(|| ErrorKind::Migration)
    }

    /// Get a reference to the underlying connection pool.
    ///
    /// This is useful for running custom queries or transactions.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Close the database connection pool.
    ///
    /// This waits for all connections to be returned to the pool and then
    /// closes them. After calling this, the Database instance should not
    /// be used.
    pub async fn close(&self) {
        // Let SQLite update query planner statistics
        _ = sqlx::query("PRAGMA optimize").execute(&self.pool).await;
        self.pool.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connect_in_memory() {
        let db = Database::connect_in_memory().await.unwrap();
        assert!(!db.pool().is_closed());
        db.close().await;
    }

    #[tokio::test]
    async fn test_migrations_are_idempotent() {
        let db = Database::connect_in_memory().await.unwrap();
        // Running migrate again should succeed (already applied)
        db.migrate().await.unwrap();
        db.close().await;
    }

    #[tokio::test]
    async fn test_pragmas_are_applied() {
        let db = Database::connect_in_memory().await.unwrap();
        // Verify a PRAGMA set by SqliteConnectOptions
        let row: (i64,) = sqlx::query_as("PRAGMA foreign_keys").fetch_one(db.pool()).await.unwrap();
        assert_eq!(row.0, 1, "foreign_keys should be ON");
        // Verify a PRAGMA set by after_connect().
        let row: (i64,) = sqlx::query_as("PRAGMA wal_autocheckpoint").fetch_one(db.pool()).await.unwrap();
        assert_eq!(row.0, 800, "WAL checkpoint should be 800");
        db.close().await;
    }
}
