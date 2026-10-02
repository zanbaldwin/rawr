//! Audiobook construction.
//!
//! Turns a [`Document`](rawr_source::Document) into an EPUB 3 with Media
//! Overlays. The [`script`] module decides what is spoken and how the text is
//! grouped into narration batches.

pub mod error;
pub mod plan;
pub mod script;
pub mod store;
