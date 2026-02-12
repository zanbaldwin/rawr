//! Compression and decompression abstractions.
//!
//! This crate provides format detection, compression, and decompression
//! utilities for HTML files.

pub mod cli;
mod construct;
pub mod error;
mod ops;
mod peekable;
mod util;

pub use crate::error::{Error, ErrorKind, Result};
pub use crate::peekable::PeekableReader;

/// Compression format enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Compression {
    /// Uncompressed
    None,
    /// Brotli compression (.br)
    #[cfg(feature = "brotli")]
    Brotli,
    /// Bzip2 compression (.bz2)
    #[default]
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
        assert_eq!(Compression::default(), Compression::Bzip2);
    }
}
