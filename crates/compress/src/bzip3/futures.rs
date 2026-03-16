//! Y'know, not having to implement state machines was the whole reason I
//! included `async-compression` as a dependency in the first place...
//!
//! I honestly cannot be bothered to deal with this, so Claude's gonna fill out
//! the rest of this file. Need some structd for both async reading and writing:
//! - impl futures::io::AsyncRead + Send + Unpin
//! - impl futures::io::AsyncWrite + Send + Unpin
//!
//! Apparently compression is an inherently synchronous thing, so just buffer
//! the whole input and perform sync operations while wrapping in async traits.
//! We're just gonna sweep under the rug and pretend the whole thing isn't the
//! Rust equivalent of a smiley face slapped on a shiny turd.

use super::{Bz3Decoder, Bz3Encoder, DEFAULT_BLOCK_SIZE};
use ::futures::io::{AsyncRead, AsyncWrite, Cursor as AsyncCursor};
use std::io::{Cursor, Error as IoError, Read, Result as IoResult, Write};
use std::pin::Pin;
use std::task::{Context, Poll, ready};

enum DecoderState<R> {
    Reading { reader: R, buf: Vec<u8> },
    Done(AsyncCursor<Vec<u8>>),
}

pub(crate) struct AsyncBz3Decoder<R> {
    inner: DecoderState<R>,
}
impl<R: AsyncRead + Send + Unpin> AsyncBz3Decoder<R> {
    pub(crate) fn new(reader: R) -> Self {
        Self {
            inner: DecoderState::Reading { reader, buf: Vec::new() },
        }
    }
}
impl<R: AsyncRead + Send + Unpin> AsyncRead for AsyncBz3Decoder<R> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<IoResult<usize>> {
        let this = self.get_mut();
        loop {
            match &mut this.inner {
                DecoderState::Reading { reader, buf: compressed } => {
                    let mut tmp = [0u8; 8192];
                    let n = ready!(Pin::new(reader).poll_read(cx, &mut tmp))?;
                    if n > 0 {
                        compressed.extend_from_slice(&tmp[..n]);
                        continue;
                    }
                    let data = std::mem::take(compressed);
                    let decompressed = if data.is_empty() {
                        Vec::new()
                    } else {
                        let mut decoder = Bz3Decoder::new(Cursor::new(data))?;
                        let mut out = Vec::new();
                        decoder.read_to_end(&mut out)?;
                        out
                    };
                    this.inner = DecoderState::Done(AsyncCursor::new(decompressed));
                    continue;
                },
                DecoderState::Done(cursor) => return Pin::new(cursor).poll_read(cx, buf),
            }
        }
    }
}

enum EncoderState<W> {
    Buffering { writer: W, data: Vec<u8> },
    Writing { writer: W, compressed: Vec<u8>, pos: usize },
    Done,
}

pub(crate) struct AsyncBz3Encoder<W> {
    inner: EncoderState<W>,
}
impl<W: AsyncWrite + Send + Unpin> AsyncBz3Encoder<W> {
    pub(crate) fn new(writer: W) -> Self {
        Self {
            inner: EncoderState::Buffering { writer, data: Vec::new() },
        }
    }
}
impl<W: AsyncWrite + Send + Unpin> AsyncWrite for AsyncBz3Encoder<W> {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        let this = self.get_mut();
        match &mut this.inner {
            EncoderState::Buffering { data, .. } => {
                data.extend_from_slice(buf);
                Poll::Ready(Ok(buf.len()))
            },
            _ => Poll::Ready(Err(IoError::other("write after close"))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let this = self.get_mut();
        loop {
            match &mut this.inner {
                EncoderState::Buffering { .. } => {
                    let EncoderState::Buffering { writer, data } =
                        std::mem::replace(&mut this.inner, EncoderState::Done)
                    else {
                        unreachable!()
                    };
                    let mut output = Vec::with_capacity(data.len());
                    let mut encoder = Bz3Encoder::new(&mut output, DEFAULT_BLOCK_SIZE)?;
                    encoder.write_all(&data)?;
                    encoder.finish()?;
                    drop(encoder);

                    this.inner = EncoderState::Writing { writer, compressed: output, pos: 0 };
                    continue;
                },
                EncoderState::Writing { writer, compressed, pos } => {
                    while *pos < compressed.len() {
                        *pos += ready!(Pin::new(&mut *writer).poll_write(cx, &compressed[*pos..]))?;
                    }
                    ready!(Pin::new(&mut *writer).poll_close(cx))?;
                    this.inner = EncoderState::Done;
                    return Poll::Ready(Ok(()));
                },
                EncoderState::Done => return Poll::Ready(Ok(())),
            }
        }
    }
}
