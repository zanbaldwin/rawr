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
