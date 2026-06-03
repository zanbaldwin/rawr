pub(crate) mod conflict;
pub mod error;
pub mod import;
pub mod organize;
pub mod scan;
mod template;

pub use crate::conflict::trash;
pub use crate::template::PathGenerator;
use rawr_compress::Compression;
use rawr_storage::BackendHandle;

/// Maximum number of files being concurrently processed.
///
/// This number was tested at 30, 100, 250, 500, etc and since this application
/// is mostly IO-bound, it doesn't have any meaningful affect on how fast
/// operations complete.
///
/// **However,** it does have a noticable effect on the _appearance_ of speed.
///
/// At high numbers (eg, 250 and 500) the progress would speed up and slow down
/// as "waves" of individual operations completed, but when limited to lower
/// numbers (eg, 30 and 50) the progress speed would be more consistent and
/// therefore appear to be completing faster.
pub const RECOMMENDED_MAX_CONCURRENCY: usize = 50;

/// Shared configuration for a file importing/organizing passes.
///
/// Bundles the [`PathGenerator`] template, optional desired [`Compression`]
/// format, and an optional trash [`BackendHandle`] used to preserve
/// irreconcilable duplicates instead of permanently discarding them.
pub struct Context {
    template: PathGenerator,
    compression: Option<Compression>,
    trash: Option<BackendHandle>,
    dry_run: bool,
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
        dry_run: bool,
    ) -> Self {
        Self {
            template,
            compression: compression.into(),
            trash: trash.into(),
            dry_run,
        }
    }
}
