pub mod error;
pub mod organize;
pub mod scan;
mod template;

pub use crate::template::PathGenerator;
pub use crate::template::{DEFAULT_TEMPLATE_EXPORT, DEFAULT_TEMPLATE_IMPORT};
use rawr_compress::Compression;
use rawr_storage::BackendHandle;

/// Maximum number of files being concurrently processed. Futures beyond this
/// limit are queued in memory and promoted as in-flight extractions complete.
pub(crate) const MAX_PROCESS_CONCURRENCY: usize = 100;

/// Shared configuration for a file importing/organizing passes.
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
