pub mod backend;
pub mod error;
mod models;
mod path;

pub use crate::backend::StorageBackend;
pub use crate::models::FileInfo;
pub use crate::path::validate as validate_path;
use std::sync::Arc;

pub type BackendHandle = Arc<dyn StorageBackend + Send + Sync>;
