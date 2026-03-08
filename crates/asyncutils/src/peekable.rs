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
    use futures::io::Cursor as AsyncCursor;

    fn test_data() -> Vec<u8> {
        b"Hello, world! This is test data for async peekable decompression. \
          It needs to be long enough to test multiple peek() calls."
            .to_vec()
    }

    #[tokio::test]
    async fn test_peek() {
        let data = test_data();
        let mut peekable = PeekableReader::new(AsyncCursor::new(data));
        let prefix = peekable.peek(5).await.unwrap();
        assert_eq!(prefix, b"Hello");
    }

    #[tokio::test]
    async fn test_peek_then_into_bytes() {
        let data = test_data();
        let original = data.clone();
        let mut peekable = PeekableReader::new(AsyncCursor::new(data));
        let prefix = peekable.peek(13).await.unwrap();
        assert_eq!(prefix, b"Hello, world!");
        let full = peekable.into_bytes().await.unwrap();
        assert_eq!(full, original);
    }

    #[tokio::test]
    async fn test_peek_then_into_reader() {
        let data = test_data();
        let original = data.clone();
        let mut peekable = PeekableReader::new(AsyncCursor::new(data));
        let prefix = peekable.peek(5).await.unwrap();
        assert_eq!(prefix, b"Hello");
        let mut output = Vec::new();
        async_copy(&mut peekable.into_reader(), &mut output).await.unwrap();
        assert_eq!(output, original);
    }

    #[tokio::test]
    async fn test_multiple_peek_calls() {
        let data = test_data();
        let original = data.clone();
        let mut peekable = PeekableReader::new(AsyncCursor::new(data));
        let prefix1 = peekable.peek(5).await.unwrap();
        assert_eq!(prefix1, b"Hello");
        let prefix2 = peekable.peek(13).await.unwrap();
        assert_eq!(prefix2, b"Hello, world!");
        assert_eq!(peekable.head(), b"Hello, world!");
        let full = peekable.into_bytes().await.unwrap();
        assert_eq!(full, original);
    }

    #[tokio::test]
    async fn test_peek_larger_than_data() {
        let data = b"tiny".to_vec();
        let mut peekable = PeekableReader::new(AsyncCursor::new(data));
        let prefix = peekable.peek(1000).await.unwrap();
        assert_eq!(prefix, b"tiny");
    }

    #[tokio::test]
    async fn test_empty_input() {
        let data = Vec::new();
        let mut peekable = PeekableReader::new(AsyncCursor::new(data));
        let prefix = peekable.peek(100).await.unwrap();
        assert!(prefix.is_empty());
        let full = peekable.into_bytes().await.unwrap();
        assert!(full.is_empty());
    }

    #[tokio::test]
    async fn test_drop_without_into_bytes() {
        let data = test_data();
        let mut peekable = PeekableReader::new(AsyncCursor::new(data));
        let _prefix = peekable.peek(5).await.unwrap();
        drop(peekable);
    }
}
