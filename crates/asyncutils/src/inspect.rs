//! Transparent byte-observation adapters for async I/O.
//!
//! > Taken from Tokio's structs of the same name, because I don't want to rely
//! > on Tokio as a dependency. Because reasons.
//!
//! [`InspectReader`] and [`InspectWriter`] pass all bytes through unchanged
//! while calling a closure on each chunk — useful for computing hashes, counting
//! bytes, or logging without breaking the streaming pipeline.
//!
//! Async counterpart of the `inspect` concept from iterators, applied to
//! [`futures::io::AsyncRead`] and [`futures::io::AsyncWrite`].

use futures::io::{AsyncRead, AsyncWrite};
use std::io;
use std::pin::Pin;
use std::task::{self, Poll};

pin_project_lite::pin_project! {
    /// An [`AsyncRead`] adapter that observes each chunk of bytes read.
    ///
    /// The closure `F` is called with a reference to each successfully-read slice.
    /// It cannot modify the data — it only observes.
    ///
    /// Because [`futures::io::copy`] borrows the reader (`&mut R`), captures in `F`
    /// remain accessible after the copy completes — no `Arc<Mutex<>>` needed.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let mut hasher = blake3::Hasher::new();
    /// let mut reader = InspectReader::new(source, |bytes| hasher.update(bytes));
    /// futures::io::copy(&mut reader, &mut writer).await?;
    /// let hash = hasher.finalize(); // ✓ still accessible
    /// ```
    pub struct InspectReader<R, F> {
        #[pin]
        inner: R,
        f: F,
    }
}
impl<R, F> InspectReader<R, F> {
    pub fn new(inner: R, f: F) -> Self {
        Self { inner, f }
    }
}
impl<R, F> AsyncRead for InspectReader<R, F>
where
    R: AsyncRead,
    F: FnMut(&[u8]),
{
    fn poll_read(self: Pin<&mut Self>, cx: &mut task::Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        let this = self.project();
        let poll = this.inner.poll_read(cx, buf);
        if let Poll::Ready(Ok(n)) = &poll {
            (this.f)(&buf[..*n]);
        }
        poll
    }
}

pin_project_lite::pin_project! {
    /// An [`AsyncWrite`] adapter that observes each chunk of bytes written.
    ///
    /// The closure `F` is called with the slice that was actually accepted by the
    /// inner writer (i.e., `&buf[..n]` where `n` is the number of bytes written).
    ///
    /// > **Ownership caveat:**
    /// > `Arc<Mutex<_>>` is required because the InspectWriter is usually the first
    /// > writer in a chain (the storage writer being the last), and ownership of
    /// > InspectWriter is taken by whatever wraps it (usually compression).
    /// >
    /// > If you need to access closure state after the wrapping layer is dropped,
    /// > use shared state (`Arc<Mutex<_>>`) in the closure's captures.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let hasher = Arc::new(Mutex::new(blake3::Hasher::new()));
    /// let size = Arc::new(Mutex::new(0u64));
    /// let writer = InspectWriter::new(backend_writer, {
    ///     let hasher = hasher.clone();
    ///     let size = size.clone();
    ///     move |bytes| {
    ///         hasher.lock().unwrap().update(bytes);
    ///         *size.lock().unwrap() += bytes.len() as u64;
    ///     }
    /// });
    /// // writer can now be consumed by async_wrap_writer
    /// // hasher and size remain accessible via the Arc handles
    /// ```
    pub struct InspectWriter<W, F> {
        #[pin]
        inner: W,
        f: F,
    }
}
impl<W, F> InspectWriter<W, F> {
    pub fn new(inner: W, f: F) -> Self {
        Self { inner, f }
    }
}
impl<W, F> AsyncWrite for InspectWriter<W, F>
where
    W: AsyncWrite,
    F: FnMut(&[u8]),
{
    fn poll_write(self: Pin<&mut Self>, cx: &mut task::Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.project();
        let poll = this.inner.poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &poll {
            (this.f)(&buf[..*n]);
        }
        poll
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_close(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::{AsyncReadExt, AsyncWriteExt, Cursor, copy as async_copy};

    #[tokio::test]
    async fn test_inspect_reader_observes_all_bytes() {
        let data = b"Hello, world! This is test data for the inspect reader.";
        let mut observed = Vec::new();
        let mut reader = InspectReader::new(Cursor::new(data.as_slice()), |bytes: &[u8]| {
            observed.extend_from_slice(bytes);
        });

        let mut output = Vec::new();
        reader.read_to_end(&mut output).await.unwrap();

        assert_eq!(output, data);
        assert_eq!(observed, data);
    }

    #[tokio::test]
    async fn test_inspect_reader_with_copy() {
        let data = b"Streaming through copy";
        let mut byte_count = 0usize;
        let mut reader = InspectReader::new(Cursor::new(data.as_slice()), |bytes: &[u8]| {
            byte_count += bytes.len();
        });

        let mut output = Cursor::new(Vec::new());
        async_copy(&mut reader, &mut output).await.unwrap();

        // Captures survive because copy borrows &mut
        assert_eq!(byte_count, data.len());
        assert_eq!(output.into_inner(), data);
    }

    #[tokio::test]
    async fn test_inspect_writer_observes_all_bytes() {
        let data = b"Hello, world! This is test data for the inspect writer.";
        let mut observed = Vec::new();
        let mut writer = InspectWriter::new(Cursor::new(Vec::new()), |bytes: &[u8]| {
            observed.extend_from_slice(bytes);
        });

        writer.write_all(data).await.unwrap();
        writer.close().await.unwrap();

        assert_eq!(observed, data);
    }

    #[tokio::test]
    async fn test_inspect_reader_empty_input() {
        let mut called = false;
        let mut reader = InspectReader::new(Cursor::new(b"".as_slice()), |_bytes: &[u8]| {
            called = true;
        });

        let mut output = Vec::new();
        reader.read_to_end(&mut output).await.unwrap();

        assert!(output.is_empty());
        assert!(!called);
    }

    #[tokio::test]
    async fn test_inspect_writer_empty_write() {
        let mut observed_total = 0usize;
        let mut writer = InspectWriter::new(Cursor::new(Vec::new()), |bytes: &[u8]| {
            observed_total += bytes.len();
        });

        writer.write_all(b"").await.unwrap();
        writer.close().await.unwrap();

        assert_eq!(observed_total, 0);
    }
}
