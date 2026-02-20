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
use crate::PathGenerator;
use rawr_compress::Compression;
use rawr_storage::BackendHandle;

/// Shared configuration for a file organization pass.
///
/// Bundles the [`PathGenerator`] template, optional desired [`Compression`]
/// format, and an optional trash [`BackendHandle`] used to preserve
/// irreconcilable duplicates instead of permanently discarding them.
pub struct Context {
    template: PathGenerator,
    compression: Option<Compression>,
    trash: Option<BackendHandle>,
}
impl Context {
    /// Creates a new organization context.
    ///
    /// `compression` sets the desired output format — files stored with a
    /// different format will be decompressed and re-compressed during the
    /// move. Pass `None` to keep each file's existing compression; don't
    /// confuse with `Some(Compression::None)` which removes compression.
    ///
    /// `trash` is an optional storage backend where irreconcilable
    /// duplicates are written before deletion.
    pub fn new(
        template: PathGenerator,
        compression: impl Into<Option<Compression>>,
        trash: impl Into<Option<BackendHandle>>,
    ) -> Self {
        Self {
            template,
            compression: compression.into(),
            trash: trash.into(),
        }
    }
}
