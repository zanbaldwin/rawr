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
