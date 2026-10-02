//! Content input for narration.
//!
//! A [`Source`] turns input bytes into a [`Document`]: a source-neutral book
//! model of metadata and XHTML sections. The [`Origin`] carries whatever the
//! source knows beyond that (for AO3, the full extracted [`Version`]), so
//! callers at the edges keep full fidelity while the narration pipeline only
//! ever reads [`DocumentMeta`] and [`Section`]s.
//!
//! [`Version`]: rawr_extract::models::Version

#[cfg(feature = "ao3")]
mod ao3;
mod document;
pub mod error;
mod source;

#[cfg(feature = "ao3")]
pub use crate::ao3::Ao3Html;
pub use crate::document::{Document, DocumentMeta, Origin, Rating, Section};
pub use crate::source::Source;
