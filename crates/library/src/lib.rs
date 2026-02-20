pub mod error;
pub mod organize;
pub mod scan;
mod template;

pub use crate::template::PathGenerator;
pub use crate::template::{DEFAULT_TEMPLATE_EXPORT, DEFAULT_TEMPLATE_IMPORT};

/// Maximum number of files being concurrently processed. Futures beyond this
/// limit are queued in memory and promoted as in-flight extractions complete.
pub(crate) const MAX_PROCESS_CONCURRENCY: usize = 100;
