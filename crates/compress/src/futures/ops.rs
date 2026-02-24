//! Async Compression Operations (feature-gated behind `async`)

use crate::Compression;
use crate::error::{ErrorKind, Result};
use async_compression::Level;
#[cfg(feature = "brotli")]
use async_compression::futures::{bufread::BrotliDecoder, write::BrotliEncoder};
use async_compression::futures::{bufread::BzDecoder, write::BzEncoder};
use async_compression::futures::{bufread::GzipDecoder, write::GzipEncoder};
#[cfg(feature = "xz")]
use async_compression::futures::{bufread::XzDecoder, write::XzEncoder};
#[cfg(feature = "zstd")]
use async_compression::futures::{bufread::ZstdDecoder, write::ZstdEncoder};
use exn::ResultExt;
use futures::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use futures::io::{BufReader as AsyncBufReader, copy as async_copy};

// I still haven't wrapped my head around the whole Unpin thing. It's a async
// reader/writer but it's unpinnable, which means it's not async? At least that
// means it's not a future-slash-state-machine. Anyway, `futures::io::copy()`
// required it, so YOU get an Unpin, and YOU get an Unpin!

impl Compression {
    /// Wrap an async reader with the appropriate decompression layer.
    /// Automatically wraps with a buffered reader internally.
    ///
    /// Async counterpart of [`Compression::wrap_reader`]. Does not throw an
    /// [`Encoder`](crate::error::ErrorKind::Encoder) error like it's sync
    /// counterpart because the underlying crate defers them until the
    /// first read attempt.
    pub fn async_wrap_reader<'a, R: AsyncRead + Unpin + 'a>(&self, reader: R) -> Box<dyn AsyncRead + Unpin + 'a> {
        // `async-compression` requires AsyncBufRead, but AsyncBufRead/AsyncWrite
        // doesn't mirror the sync API of Read/Write. Wrap the incoming AsyncRead
        // in a buffered version, so the callee doesn't need to do it.
        let reader = AsyncBufReader::new(reader);
        match self {
            Compression::None => Box::new(reader),
            #[cfg(feature = "brotli")]
            Compression::Brotli => Box::new(BrotliDecoder::new(reader)),
            Compression::Bzip2 => Box::new(BzDecoder::new(reader)),
            Compression::Gzip => Box::new(GzipDecoder::new(reader)),
            #[cfg(feature = "xz")]
            Compression::Xz => Box::new(XzDecoder::new(reader)),
            #[cfg(feature = "zstd")]
            Compression::Zstd => Box::new(ZstdDecoder::new(reader)),
        }
    }

    /// Wrap an async writer with the appropriate compression layer.
    ///
    /// Async counterpart of [`Compression::wrap_writer`].  Does not throw an
    /// [`Encoder`](crate::error::ErrorKind::Encoder) error like it's sync
    /// counterpart because the underlying crate defers them until the
    /// first write attempt.
    ///
    /// The caller **must** call [`AsyncWriteExt::close`] on the returned writer
    /// to finalize the compressed stream.
    pub fn async_wrap_writer<'a, W: AsyncWrite + Unpin + 'a>(&self, writer: W) -> Box<dyn AsyncWrite + Unpin + 'a> {
        match self {
            Compression::None => Box::new(writer),
            #[cfg(feature = "brotli")]
            Compression::Brotli => Box::new(BrotliEncoder::with_quality(writer, Level::Best)),
            Compression::Bzip2 => Box::new(BzEncoder::with_quality(writer, Level::Best)),
            Compression::Gzip => Box::new(GzipEncoder::with_quality(writer, Level::Best)),
            #[cfg(feature = "xz")]
            Compression::Xz => Box::new(XzEncoder::with_quality(writer, Level::Best)),
            #[cfg(feature = "zstd")]
            Compression::Zstd => Box::new(ZstdEncoder::with_quality(writer, Level::Precise(22))),
        }
    }

    /// Compress from an async reader into an async writer, returning bytes copied.
    /// Automatically wraps the reader in a buffer internally.
    ///
    /// Async counterpart of [`Compression::compress_stream`].
    pub async fn async_compress_stream<R, W>(&self, reader: &mut R, writer: &mut W) -> Result<u64>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        // `async-compression` is just my lazy syntactic sugar to wrap sync encoders/decoders,
        // because I don't want to have to do it myself. But compression formats are frame-based,
        // and if you don't use buffers you're gonna have a bad time, m'kay?
        // So don't be silly and wrap your possibly-unbuffered-async-input.
        // Also, the crate only accepts buffered ones, so you don't have a choice.
        let reader = AsyncBufReader::new(reader);
        let mut writer = self.async_wrap_writer(writer);
        let bytes = async_copy(reader, &mut writer).await.or_raise(|| ErrorKind::Io)?;
        writer.close().await.or_raise(|| ErrorKind::Io)?;
        Ok(bytes)
    }

    /// Decompress from an async buffered reader to an async writer, returning bytes copied.
    /// Automatically wraps the reader in a buffer internally.
    ///
    /// Async counterpart of [`Compression::decompress_stream`].
    pub async fn async_decompress_stream<R, W>(&self, reader: &mut R, writer: &mut W) -> Result<u64>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let reader = self.async_wrap_reader(reader);
        let bytes = async_copy(reader, writer).await.or_raise(|| ErrorKind::Io)?;
        writer.close().await.or_raise(|| ErrorKind::Io)?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use crate::Compression;
    use futures::io::{AsyncReadExt, AsyncWriteExt, BufReader, Cursor};
    use rstest::rstest;

    #[tokio::test]
    #[rstest]
    #[case(Compression::Bzip2)]
    #[case(Compression::Gzip)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    async fn test_async_wrap_reader(#[case] format: Compression) {
        let original = b"Hello, world!";
        let compressed = format.compress(original).unwrap();
        assert_ne!(compressed, original);
        let cursor = BufReader::new(Cursor::new(compressed));
        let mut reader = format.async_wrap_reader(cursor);
        let mut decompressed = Vec::new();
        reader.read_to_end(&mut decompressed).await.unwrap();
        assert_eq!(decompressed, original);
    }

    #[tokio::test]
    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Bzip2)]
    #[case(Compression::Gzip)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    async fn test_async_wrap_writer(#[case] format: Compression) {
        let original = b"Hello, world! This is a test of async compression.";
        let mut compressed = Vec::new();
        let mut writer = format.async_wrap_writer(Cursor::new(&mut compressed));
        writer.write_all(original).await.unwrap();
        writer.close().await.unwrap();
        drop(writer);
        assert!(!compressed.is_empty());
    }

    #[tokio::test]
    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Bzip2)]
    #[case(Compression::Gzip)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    async fn test_async_stream_roundtrip(#[case] format: Compression) {
        let original = b"Hello, world! This is a test of async streaming compression.";

        let mut compressed = Cursor::new(Vec::new());
        let bytes_in = format.async_compress_stream(&mut Cursor::new(original), &mut compressed).await.unwrap();
        assert_eq!(bytes_in, original.len() as u64);

        let compressed = compressed.into_inner();
        let mut decompressed = Cursor::new(Vec::new());
        let mut reader = BufReader::new(Cursor::new(compressed));
        let bytes_out = format.async_decompress_stream(&mut reader, &mut decompressed).await.unwrap();
        assert_eq!(bytes_out, original.len() as u64);
        assert_eq!(decompressed.into_inner(), original);
    }
}
