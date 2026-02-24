use crate::Compression;
use crate::error::{ErrorKind, Result};
use exn::ResultExt;
use futures::io::copy as async_copy;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use futures::io::{BufReader as AsyncBufReader, Chain as AsyncChain, Cursor as AsyncCursor};

pub struct AsyncPeekableReader<R> {
    decoder: R,
    buffer: Vec<u8>,
}

impl<R: AsyncRead + Unpin> AsyncPeekableReader<R> {
    pub fn new(decoder: R) -> Self {
        Self { decoder, buffer: Vec::new() }
    }

    pub async fn peek(&mut self, limit: usize) -> Result<&[u8]> {
        if self.buffer.len() >= limit {
            return Ok(&self.buffer[..limit]);
        }
        let needed = (limit - self.buffer.len()) as u64;
        (&mut self.decoder).take(needed).read_to_end(&mut self.buffer).await.or_raise(|| ErrorKind::InvalidData)?;
        Ok(&self.buffer[..self.buffer.len().min(limit)])
    }

    pub fn head(&self) -> &[u8] {
        &self.buffer
    }

    pub fn into_reader(self) -> AsyncChain<AsyncCursor<Vec<u8>>, R> {
        AsyncCursor::new(self.buffer).chain(self.decoder)
    }

    pub async fn into_bytes(mut self) -> Result<Vec<u8>> {
        self.decoder.read_to_end(&mut self.buffer).await.or_raise(|| ErrorKind::InvalidData)?;
        Ok(self.buffer)
    }

    pub async fn copy_into<W: AsyncWrite + Unpin>(self, writer: &mut W) -> Result<u64> {
        async_copy(&mut self.into_reader(), writer).await.or_raise(|| ErrorKind::Io)
    }
}

impl Compression {
    pub fn async_peekable_reader<'a, R: AsyncRead + Unpin + 'a>(
        &self,
        reader: R,
    ) -> Result<AsyncPeekableReader<Box<dyn AsyncRead + Unpin + 'a>>> {
        Ok(AsyncPeekableReader::new(self.async_wrap_reader(reader)))
    }

    pub fn async_peekable_data<'a>(
        &self,
        input: &'a [u8],
    ) -> Result<AsyncPeekableReader<Box<dyn AsyncRead + Unpin + 'a>>> {
        self.async_peekable_reader(AsyncBufReader::new(input))
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

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    #[tokio::test]
    async fn test_async_peek(#[case] format: Compression) {
        let original = test_data();
        let compressed = format.compress(&original).unwrap();
        let mut peekable = format.async_peekable_data(&compressed).unwrap();
        let prefix = peekable.peek(5).await.unwrap();
        assert_eq!(prefix, b"Hello");
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    #[tokio::test]
    async fn test_async_peek_then_into_bytes(#[case] format: Compression) {
        let original = test_data();
        let compressed = format.compress(&original).unwrap();
        let mut peekable = format.async_peekable_data(&compressed).unwrap();
        let prefix = peekable.peek(13).await.unwrap();
        assert_eq!(prefix, b"Hello, world!");
        let full = peekable.into_bytes().await.unwrap();
        assert_eq!(full, original);
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    #[tokio::test]
    async fn test_async_multiple_peek_calls(#[case] format: Compression) {
        let original = test_data();
        let compressed = format.compress(&original).unwrap();
        let mut peekable = format.async_peekable_data(&compressed).unwrap();
        let prefix1 = peekable.peek(5).await.unwrap();
        assert_eq!(prefix1, b"Hello");
        let prefix2 = peekable.peek(13).await.unwrap();
        assert_eq!(prefix2, b"Hello, world!");
        assert_eq!(peekable.head(), b"Hello, world!");
        let full = peekable.into_bytes().await.unwrap();
        assert_eq!(full, original);
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    #[tokio::test]
    async fn test_async_peek_larger_than_data(#[case] format: Compression) {
        let original = b"tiny";
        let compressed = format.compress(original).unwrap();
        let mut peekable = format.async_peekable_data(&compressed).unwrap();
        let prefix = peekable.peek(1000).await.unwrap();
        assert_eq!(prefix, b"tiny");
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    #[tokio::test]
    async fn test_async_empty_input(#[case] format: Compression) {
        let original = b"";
        let compressed = format.compress(original).unwrap();
        let mut peekable = format.async_peekable_data(&compressed).unwrap();
        let prefix = peekable.peek(100).await.unwrap();
        assert!(prefix.is_empty());
        let full = peekable.into_bytes().await.unwrap();
        assert!(full.is_empty());
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    #[tokio::test]
    async fn test_async_copy_into(#[case] format: Compression) {
        let original = test_data();
        let compressed = format.compress(&original).unwrap();
        let mut peekable = format.async_peekable_data(&compressed).unwrap();
        let prefix = peekable.peek(5).await.unwrap();
        assert_eq!(prefix, b"Hello");
        let mut output = futures::io::Cursor::new(Vec::new());
        let bytes = peekable.copy_into(&mut output).await.unwrap();
        assert_eq!(bytes, original.len() as u64);
        assert_eq!(output.into_inner(), original);
    }

    #[tokio::test]
    async fn test_async_drop_without_into_bytes() {
        let original = test_data();
        let compressed = Compression::Gzip.compress(&original).unwrap();
        let mut peekable = Compression::Gzip.async_peekable_data(&compressed).unwrap();
        let _prefix = peekable.peek(5).await.unwrap();
        drop(peekable);
    }
}
