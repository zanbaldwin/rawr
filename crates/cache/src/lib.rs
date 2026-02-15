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

pub mod error;
