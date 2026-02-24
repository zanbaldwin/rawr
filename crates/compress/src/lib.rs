//! Compression and decompression abstractions.
//!
//! This crate provides format detection, compression, and decompression
//! utilities for HTML files.

pub mod cli;
mod construct;
pub mod error;
#[cfg(feature = "async")]
mod futures;
mod ops;
mod peekable;
mod util;

pub use crate::error::{Error, ErrorKind, Result};
#[cfg(feature = "async")]
pub use crate::futures::peekable::AsyncPeekableReader;
pub use crate::peekable::PeekableReader;

/// Compression format enum.
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
