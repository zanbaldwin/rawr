pub mod backend;
pub mod error;
pub mod file;
mod path;

use crate::backend::StorageBackend;
pub use crate::path::validate as validate_path;
use std::sync::Arc;

pub type BackendHandle = Arc<dyn StorageBackend + Send + Sync>;
