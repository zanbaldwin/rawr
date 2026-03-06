//! Async peekable (partial decompression) for resumable workflows.
//!
//! Async counterpart of [`crate::PeekableReader`], using
//! [`futures::io::AsyncRead`] instead of [`std::io::Read`].

use futures::io::copy as async_copy;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use futures::io::{Chain as AsyncChain, Cursor as AsyncCursor};
use std::io::Error as IoError;

/// An async resumable reader for peek-decide-stream workflows.
///
/// Read enough decompressed data to inspect (e.g., HTML `<head>` metadata),
/// then either stream the full content onward via [`into_reader`](Self::into_reader),
/// [`into_bytes`](Self::into_bytes), [`copy_into`](Self::copy_into), or drop
/// to discard.
pub struct PeekableReader<R> {
    decoder: R,
    buffer: Vec<u8>,
}

impl<R: AsyncRead + Unpin> PeekableReader<R> {
    /// Wrap any async reader for peeking.
    pub fn new(decoder: R) -> Self {
        Self { decoder, buffer: Vec::new() }
    }

    /// Read up to `limit` bytes of the decompressed content.
    ///
    /// Behaves identically to [`PeekableReader::peek`](crate::PeekableReader::peek)
    /// (successive calls accumulate into the same buffer).
    pub async fn peek(&mut self, limit: usize) -> Result<&[u8], IoError> {
        if self.buffer.len() >= limit {
            return Ok(&self.buffer[..limit]);
        }
        let needed = (limit - self.buffer.len()) as u64;
        (&mut self.decoder).take(needed).read_to_end(&mut self.buffer).await?;
        Ok(&self.buffer[..self.buffer.len().min(limit)])
    }

    /// Access data read into internal buffer so far.
    pub fn head(&self) -> &[u8] {
        &self.buffer
    }

    /// Convert into an [`AsyncRead`]er that replays the buffered head, then
    /// streams remaining decoder output.
    ///
    /// Async counterpart of [`PeekableReader::into_reader`](crate::PeekableReader::into_reader).
    pub fn into_reader(self) -> AsyncChain<AsyncCursor<Vec<u8>>, R> {
        AsyncCursor::new(self.buffer).chain(self.decoder)
    }

    /// Read all remaining data and return the complete buffer.
    pub async fn into_bytes(mut self) -> Result<Vec<u8>, IoError> {
        self.decoder.read_to_end(&mut self.buffer).await?;
        Ok(self.buffer)
    }

    /// Stream all data (buffered plus unbuffered) into the specified
    /// async writer. Works well with [`Compression::async_wrap_writer`].
    pub async fn copy_into<W: AsyncWrite + Unpin>(self, writer: &mut W) -> Result<u64, IoError> {
        async_copy(&mut self.into_reader(), writer).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn test_data() -> Vec<u8> {
        b"Hello, world! This is test data for async peekable decompression. \
          It needs to be long enough to test multiple peek() calls."
            .to_vec()
    }
}
