//! Shared configuration-to-runtime wiring for rawr binaries.
//!
//! Everything in here is pure *composition*: it takes values from
//! [`rawr_config`] and turns them into live runtime objects (storage
//! backends, path generators, style sets). No binary-specific concerns —
//! the CLI and any other front-end call these and then get on with their
//! own lives.

pub mod backend;
pub mod error;
#[cfg(feature = "render")]
pub mod style;
pub mod template;

pub use crate::backend::{BackendPurpose, backend_by_name, backend_by_purpose};
#[cfg(feature = "render")]
pub use crate::style::style_config;
pub use crate::template::{export_path_generator, import_path_generator, library_context, path_generator};
