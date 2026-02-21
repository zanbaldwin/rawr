//! File organization and path normalization.
//!
//! Relocates library files from their current storage paths to the "correct"
//! location determined by a [`PathGenerator`] template. Files may also be
//! re-compressed to a different [`Compression`] format during the move.
//!
//! When the target path is already occupied, the module performs recursive
//! conflict resolution — relocating the existing file first —
//! with a depth limit to prevent circular/infinite loops.
//!
//! The primary entry point is [`organize`] which pulls all known files (for the
//! specified [backend](rawr_storage)) from the [cache](rawr_cache) and streams
//! the resulting [`Action`]s from passing each discovered file to [`organize_file`]
//! (accepting any [`HashState`](rawr_storage::file::HashState)).

mod conflict;
pub mod error;
mod file;
mod stream;

pub use self::file::{Action, organize_file};
pub use self::stream::{OrganizeEvent, organize};
