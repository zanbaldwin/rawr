//! Audiobook construction.
//!
//! Turns a [`Document`](rawr_source::Document) into an EPUB 3 with Media
//! Overlays. The [`script`] module decides what is spoken and how the text is
//! grouped into narration batches.

pub mod audio;
pub mod epub3;
pub mod error;
mod export;
pub mod narrate;
pub mod plan;
pub mod script;
pub mod store;
pub mod timeline;

pub use crate::export::{ExportOptions, export, export_with_options};
