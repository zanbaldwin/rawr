//! SQLite cache database for library metadata.
//!
//! This crate provides the ephemeral cache database that tracks the current
//! known state of the library. The database is not the source of truth - the
//! HTML files themselves are. If the database is deleted, it can be rebuilt
//! by scanning the library.
//!
//! # Architecture
//! The cache stores two main entity types:
//! - **Versions**: Extracted metadata from HTML files, keyed by content hash
//!   (BLAKE3 hash of decompressed HTML). Multiple versions may exist for the
//!   same work_id if the content differs between downloads.
//! - **FileRecords**: Physical files tracked across targets, linking paths
//!   to their content hashes. Multiple files may reference the same Version
//!   if they have identical content.

mod db;
pub mod error;
mod models;
mod repo;

pub use crate::db::Database;
pub use crate::repo::Repository;
use rawr_extract::models as extract;
use rawr_storage::file as storage;

pub(crate) type File = storage::FileInfo<storage::Processed>;
pub(crate) type Version = extract::Version;
