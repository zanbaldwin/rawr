//! Peekable (partial decompression) for resumable workflows.
//!
//! Thin convenience wrapper around standard library I/O primitives
//! ([`Read::take`], [`Cursor`], [`Chain`]) for the peek-decide-stream pattern.

use crate::Compression;
use crate::error::{ErrorKind, Result};
use exn::ResultExt;
use std::io::{Chain, Cursor, Read, Write};

/// A resumable [`Read`]er for peek-decide-stream workflows.
///
/// Read enough decompressed data to inspect (e.g., HTML `<head>` metadata),
/// then either stream the full content onward via [`into_reader`](Self::into_reader),
/// [`into_bytes`](Self::into_bytes), [`copy_into`](Self::copy_into), or drop
/// to discard.
pub struct PeekableReader<R> {
    decoder: R,
    buffer: Vec<u8>,
}

impl<R: Read> PeekableReader<R> {
    /// Wrap any reader for peeking.
    pub fn new(decoder: R) -> Self {
        Self { decoder, buffer: Vec::new() }
    }

    /// Read up to `limit` bytes of the decompressed content.
    ///
    /// Returns a slice of all buffered data. Successive calls do not accumulate:
    /// - `peek(4*1024)` puts 4KiB in the buffer, returns 4KiB
    /// - `peek(8*1024)` puts an additional 4KiB in the buffer, returns 8KiB
    /// - `peek(2*1024)` immediately returns 2KiB (because buffer already has 8KiB)
    pub fn peek(&mut self, limit: usize) -> Result<&[u8]> {
        if self.buffer.len() >= limit {
            return Ok(&self.buffer[..limit]);
        }
        let needed = (limit - self.buffer.len()) as u64;
        (&mut self.decoder).take(needed).read_to_end(&mut self.buffer).or_raise(|| ErrorKind::InvalidData)?;
        Ok(&self.buffer[..self.buffer.len().min(limit)])
    }

    /// Access data read into internal buffer so far.
    pub fn head(&self) -> &[u8] {
        &self.buffer
    }

    /// Convert into a [`Read`]er that replays the buffered head, then
    /// streams remaining decoder output.
    pub fn into_reader(self) -> Chain<Cursor<Vec<u8>>, R> {
        Cursor::new(self.buffer).chain(self.decoder)
    }

    /// Read all remaining data and return the complete buffer.
    pub fn into_bytes(mut self) -> Result<Vec<u8>> {
        self.decoder.read_to_end(&mut self.buffer).or_raise(|| ErrorKind::InvalidData)?;
        Ok(self.buffer)
    }

    /// Stream all data (buffered plus unbuffered) into the specified
    /// writer. Works well with [`Compression::wrap_writer`].
    pub fn copy_into<W: Write>(self, writer: &mut W) -> Result<u64> {
        std::io::copy(&mut self.into_reader(), writer).or_raise(|| ErrorKind::Io)
    }
}

impl Compression {
    /// Create a peekable decompressor from any reader. This is the primary
    /// constructor for file-based and streaming workflows.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use rawr_compress::Compression;
    /// use std::fs::File;
    /// use std::io::{BufReader, BufWriter};
    /// use std::path::PathBuf;
    ///
    /// let source_path = PathBuf::from("path/to/file.html.bz2");
    /// let source_reader = BufReader::new(File::open(&source_path).unwrap());
    ///
    /// let mut peekable = Compression::from_path(&source_path)
    ///     .peekable_reader(source_reader)
    ///     .unwrap();
    /// let is_html5 = peekable.peek(15).unwrap() == b"<!DOCTYPE html>";
    ///
    /// if is_html5 {
    ///     let target_path = PathBuf::from("path/to/file.html");
    ///     let mut target_writer = BufWriter::new(File::create(&target_path).unwrap());
    ///     // Copy the entire decompressed contents of the source file into the target file.
    ///     peekable.copy_into(&mut target_writer).unwrap();
    /// }
    /// // Else, discard.
    /// ```
    pub fn peekable_reader<'a, R: Read + 'a>(&self, reader: R) -> Result<PeekableReader<Box<dyn Read + 'a>>> {
        Ok(PeekableReader::new(self.wrap_reader(reader)?))
    }

    /// Create a peekable decompressor from compressed bytes. Convenience
    /// wrapper over [`peekable_reader`](Self::peekable_reader) for in-memory
    /// data.
    ///
    /// # Example
    ///
    /// ```
    /// use rawr_compress::Compression;
    ///
    /// let format = Compression::Bzip2;
    /// let data = b"Some data with metadata at the start";
    /// let compressed = format.compress(data).unwrap();
    ///
    /// let mut peekable = format.peekable_data(&compressed).unwrap();
    /// let prefix = peekable.peek(9).unwrap();
    /// assert_eq!(prefix, b"Some data");
    ///
    /// let full = peekable.into_bytes().unwrap();
    /// assert_eq!(full, data);
    /// ```
    pub fn peekable_data<'a>(&self, input: &'a [u8]) -> Result<PeekableReader<Box<dyn Read + 'a>>> {
        self.peekable_reader(Cursor::new(input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn test_data() -> Vec<u8> {
        b"Hello, world! This is test data for peekable decompression. \
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
    fn test_peek(#[case] format: Compression) {
        let original = test_data();
        let compressed = format.compress(&original).unwrap();
        let mut peekable = format.peekable_data(&compressed).unwrap();
        let prefix = peekable.peek(5).unwrap();
        assert_eq!(prefix, b"Hello");
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    fn test_peek_then_into_bytes(#[case] format: Compression) {
        let original = test_data();
        let compressed = format.compress(&original).unwrap();
        let mut peekable = format.peekable_data(&compressed).unwrap();
        let prefix = peekable.peek(13).unwrap();
        assert_eq!(prefix, b"Hello, world!");
        let full = peekable.into_bytes().unwrap();
        assert_eq!(full, original);
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    fn test_peek_then_into_reader(#[case] format: Compression) {
        let original = test_data();
        let compressed = format.compress(&original).unwrap();
        let mut peekable = format.peekable_data(&compressed).unwrap();
        let prefix = peekable.peek(5).unwrap();
        assert_eq!(prefix, b"Hello");
        let mut output = Vec::new();
        std::io::copy(&mut peekable.into_reader(), &mut output).unwrap();
        assert_eq!(output, original);
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    fn test_multiple_peek_calls(#[case] format: Compression) {
        let original = test_data();
        let compressed = format.compress(&original).unwrap();
        let mut peekable = format.peekable_data(&compressed).unwrap();
        let prefix1 = peekable.peek(5).unwrap();
        assert_eq!(prefix1, b"Hello");
        let prefix2 = peekable.peek(13).unwrap();
        assert_eq!(prefix2, b"Hello, world!");
        assert_eq!(peekable.head(), b"Hello, world!");
        let full = peekable.into_bytes().unwrap();
        assert_eq!(full, original);
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    fn test_peek_larger_than_data(#[case] format: Compression) {
        let original = b"tiny";
        let compressed = format.compress(original).unwrap();
        let mut peekable = format.peekable_data(&compressed).unwrap();
        let prefix = peekable.peek(1000).unwrap();
        assert_eq!(prefix, b"tiny");
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    fn test_empty_input(#[case] format: Compression) {
        let original = b"";
        let compressed = format.compress(original).unwrap();
        let mut peekable = format.peekable_data(&compressed).unwrap();
        let prefix = peekable.peek(100).unwrap();
        assert!(prefix.is_empty());
        let full = peekable.into_bytes().unwrap();
        assert!(full.is_empty());
    }

    #[test]
    fn test_drop_without_into_bytes() {
        let original = test_data();
        let compressed = Compression::Gzip.compress(&original).unwrap();
        let mut peekable = Compression::Gzip.peekable_data(&compressed).unwrap();
        let _prefix = peekable.peek(5).unwrap();
        drop(peekable);
    }
}
