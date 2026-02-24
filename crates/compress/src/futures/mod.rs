//! Async compression and decompression operations.
//!
//! Provides async counterparts to the sync APIs in the parent module, using
//! [`futures::io`] traits (`AsyncRead`/`AsyncWrite`) rather than Tokio-specific
//! types for runtime portability.
//!
//! Requires the `async` feature.

pub(crate) mod ops;
pub(crate) mod peekable;
