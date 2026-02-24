//! Compression and decompression with automatic format detection.
//!
//! This crate wraps several compression libraries behind a unified
//! [`Compression`] enum, providing:
//!
//! - **Format detection** from file extensions ([`Compression::from_path`]) or
//!   magic bytes ([`Compression::from_magic_bytes`])
//! - **In-memory** compression/decompression ([`Compression::compress`],
//!   [`Compression::decompress`])
//! - **Streaming** via wrapped readers/writers ([`Compression::wrap_reader`],
//!   [`Compression::wrap_writer`])
//! - **Peek-decide-stream** workflows via [`PeekableReader`] — decompress just
//!   enough to inspect content, then stream the rest or discard
//!
//! Bzip2 and Gzip are always available. Optional formats (Brotli, XZ, Zstd)
//! are behind feature flags. Async counterparts require the `async` feature
//! and use [`futures`](::futures::io) traits (not Tokio).
//!
//! All compression uses the highest available level for each format,
//! prioritizing storage space over speed.

#[cfg(feature = "cli")]
pub mod cli;
mod construct;
pub mod error;
#[cfg(feature = "async")]
mod futures;
mod ops;
mod peekable;
mod util;

#[cfg(feature = "async")]
pub use crate::futures::peekable::AsyncPeekableReader;
pub use crate::peekable::PeekableReader;

/// A supported compression format.
///
/// Variants gated behind feature flags (`brotli`, `xz`, `zstd`) are only
/// available when the corresponding feature is enabled. Defaults to
/// [`None`](Self::None) (uncompressed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Compression {
    /// Uncompressed
    #[default]
    None,
    /// Brotli compression (.br)
    #[cfg(feature = "brotli")]
    Brotli,
    /// Bzip2 compression (.bz2)
    Bzip2,
    /// Gzip compression (.gz)
    Gzip,
    /// XZ/LZMA compression (.xz)
    #[cfg(feature = "xz")]
    Xz,
    /// Zstd compression (.zst)
    #[cfg(feature = "zstd")]
    Zstd,
}

#[cfg(test)]
mod tests {
    use crate::Compression;

    #[test]
    fn compression_default() {
        assert_eq!(Compression::default(), Compression::None);
    }
}
