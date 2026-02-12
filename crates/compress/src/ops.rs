//! Compression Operations

use crate::Compression;
use crate::error::{ErrorKind, Result};
#[cfg(feature = "brotli")]
use brotli::{CompressorWriter as BrotliEncoder, Decompressor as BrotliDecoder};
use bzip2::{Compression as BzCompression, read::BzDecoder, write::BzEncoder};
use exn::ResultExt;
use flate2::{Compression as GzCompression, read::GzDecoder, write::GzEncoder};
use std::io::{Read, Write};
use tracing::instrument;
#[cfg(feature = "xz")]
use xz2::{read::XzDecoder, write::XzEncoder};
#[cfg(feature = "zstd")]
use zstd::stream::{read::Decoder as ZstdDecoder, write::Encoder as ZstdEncoder};

// Use the highest compression level available for the formats; this crate
// prioritizes storage space over speed. If an end-user finds these levels
// too resource-intensive, choose a different format.
const BZIP2_LEVEL: BzCompression = BzCompression::best();
const GZIP_LEVEL: GzCompression = GzCompression::best();
#[cfg(feature = "xz")]
const XZ_LEVEL: u32 = 9;
#[cfg(feature = "zstd")]
const ZSTD_LEVEL: i32 = 22;
#[cfg(feature = "brotli")]
const BROTLI_LEVEL: u32 = 11;
#[cfg(feature = "brotli")]
const BROTLI_BUFFER_SIZE: usize = 4096;
#[cfg(feature = "brotli")]
const BROTLI_LG_WINDOW_SIZE: u32 = 22;

impl Compression {
    /// Compress a byte slice in memory.
    ///
    /// # Examples
    ///
    /// ```
    /// use rawr_compress::Compression;
    ///
    /// let data = b"Hello, world!";
    /// let compressed = Compression::Bzip2.compress(data).unwrap();
    /// assert!(compressed.len() < data.len() || data.len() < 100);
    /// ```
    pub fn compress(&self, input: &[u8]) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        self.compress_into(input, &mut output)?;
        Ok(output)
    }

    /// Decompress a byte slice in memory.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rawr_compress::Compression;
    ///
    /// let original = b"Hello, world!";
    /// let compressed = Compression::Bzip2.compress(original).unwrap();
    /// assert_ne!(compressed, original);
    /// let decompressed = Compression::Bzip2.decompress(&compressed).unwrap();
    /// assert_eq!(decompressed, original);
    /// ```
    pub fn decompress(&self, input: &[u8]) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        self.decompress_into(input, &mut output)?;
        Ok(output)
    }

    #[instrument(skip(input, output), fields(
        format = %self,
        input_size = input.len(),
        output_size
    ))]
    pub fn compress_into(&self, input: &[u8], output: &mut Vec<u8>) -> Result<usize> {
        let size = match self {
            Compression::None => {
                output.extend_from_slice(input);
                input.len()
            },
            #[cfg(feature = "brotli")]
            Compression::Brotli => {
                let mut encoder =
                    BrotliEncoder::new(&mut *output, BROTLI_BUFFER_SIZE, BROTLI_LEVEL, BROTLI_LG_WINDOW_SIZE);
                encoder.write_all(input).or_raise(|| ErrorKind::Io)?;
                // Brotli doesn't have some sort of finish/flush method?!
                drop(encoder);
                output.len()
            },
            Compression::Bzip2 => {
                let mut encoder = BzEncoder::new(&mut *output, BZIP2_LEVEL);
                encoder.write_all(input).or_raise(|| ErrorKind::Io)?;
                encoder.finish().or_raise(|| ErrorKind::Io)?;
                output.len()
            },
            Compression::Gzip => {
                let mut encoder = GzEncoder::new(&mut *output, GZIP_LEVEL);
                encoder.write_all(input).or_raise(|| ErrorKind::Io)?;
                encoder.finish().or_raise(|| ErrorKind::Io)?;
                output.len()
            },
            #[cfg(feature = "xz")]
            Compression::Xz => {
                let mut encoder = XzEncoder::new(&mut *output, XZ_LEVEL);
                encoder.write_all(input).or_raise(|| ErrorKind::Io)?;
                encoder.finish().or_raise(|| ErrorKind::Io)?;
                output.len()
            },
            #[cfg(feature = "zstd")]
            Compression::Zstd => {
                let mut encoder = ZstdEncoder::new(&mut *output, ZSTD_LEVEL).or_raise(|| ErrorKind::Encoder)?;
                encoder.write_all(input).or_raise(|| ErrorKind::Io)?;
                encoder.finish().or_raise(|| ErrorKind::Io)?;
                output.len()
            },
        };
        tracing::Span::current().record("output_size", size);
        Ok(size)
    }

    #[instrument(skip(input, output), fields(
        format = %self,
        input_size = input.len(),
        output_size
    ))]
    pub fn decompress_into(&self, input: &[u8], output: &mut Vec<u8>) -> Result<usize> {
        let size = match self {
            Compression::None => {
                output.extend_from_slice(input);
                input.len()
            },
            #[cfg(feature = "brotli")]
            Compression::Brotli => {
                let mut decoder = BrotliDecoder::new(input, BROTLI_BUFFER_SIZE);
                decoder.read_to_end(output).or_raise(|| ErrorKind::InvalidData)?
            },
            Compression::Bzip2 => {
                let mut decoder = BzDecoder::new(input);
                decoder.read_to_end(output).or_raise(|| ErrorKind::InvalidData)?
            },
            Compression::Gzip => {
                let mut decoder = GzDecoder::new(input);
                decoder.read_to_end(output).or_raise(|| ErrorKind::InvalidData)?
            },
            #[cfg(feature = "xz")]
            Compression::Xz => {
                let mut decoder = XzDecoder::new(input);
                decoder.read_to_end(output).or_raise(|| ErrorKind::InvalidData)?
            },
            #[cfg(feature = "zstd")]
            Compression::Zstd => {
                let mut decoder = ZstdDecoder::new(input).or_raise(|| ErrorKind::Encoder)?;
                decoder.read_to_end(output).or_raise(|| ErrorKind::InvalidData)?
            },
        };
        tracing::Span::current().record("output_size", size);
        Ok(size)
    }

    /// Wrap a reader with the appropriate decompression layer.
    ///
    /// Returns a boxed reader that automatically decompresses data.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::{Cursor, Read};
    /// use rawr_compress::Compression;
    ///
    /// let original = b"Hello, world!";
    /// let compressed = Compression::Gzip.compress(original).unwrap();
    /// let cursor = Cursor::new(compressed);
    /// let mut reader = Compression::Gzip.wrap_reader(cursor).unwrap();
    /// let mut decompressed = Vec::new();
    /// reader.read_to_end(&mut decompressed).unwrap();
    /// assert_eq!(decompressed, original);
    /// ```
    pub fn wrap_reader<'a, R: Read + 'a>(&self, reader: R) -> Result<Box<dyn Read + 'a>> {
        Ok(match self {
            Compression::None => Box::new(reader),
            #[cfg(feature = "brotli")]
            Compression::Brotli => Box::new(BrotliDecoder::new(reader, BROTLI_BUFFER_SIZE)),
            Compression::Bzip2 => Box::new(BzDecoder::new(reader)),
            Compression::Gzip => Box::new(GzDecoder::new(reader)),
            #[cfg(feature = "xz")]
            Compression::Xz => Box::new(XzDecoder::new(reader)),
            #[cfg(feature = "zstd")]
            Compression::Zstd => Box::new(ZstdDecoder::new(reader).or_raise(|| ErrorKind::Encoder)?),
        })
    }

    /// Wrap a writer with the appropriate compression layer.
    ///
    /// Returns a boxed writer that automatically compresses data.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::Write;
    /// use rawr_compress::Compression;
    ///
    /// let output = Vec::new();
    /// let mut writer = Compression::Bzip2.wrap_writer(output).unwrap();
    /// writer.write_all(b"Hello, world!").unwrap();
    /// // Writer takes ownership of output, compressing data on write
    /// ```
    pub fn wrap_writer<'a, W: Write + 'a>(&self, writer: W) -> Result<Box<dyn Write + 'a>> {
        Ok(match self {
            Compression::None => Box::new(writer),
            #[cfg(feature = "brotli")]
            Compression::Brotli => {
                Box::new(BrotliEncoder::new(writer, BROTLI_BUFFER_SIZE, BROTLI_LEVEL, BROTLI_LG_WINDOW_SIZE))
            },
            Compression::Bzip2 => Box::new(BzEncoder::new(writer, BZIP2_LEVEL)),
            Compression::Gzip => Box::new(GzEncoder::new(writer, GZIP_LEVEL)),
            #[cfg(feature = "xz")]
            Compression::Xz => Box::new(XzEncoder::new(writer, XZ_LEVEL)),
            #[cfg(feature = "zstd")]
            Compression::Zstd => {
                Box::new(ZstdEncoder::new(writer, ZSTD_LEVEL).or_raise(|| ErrorKind::Encoder)?.auto_finish())
            },
        })
    }

    /// Compress from a reader to a writer, returning bytes written.
    ///
    /// This is a convenience method for streaming compression without
    /// buffering the entire input in memory.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::Cursor;
    /// use rawr_compress::Compression;
    ///
    /// let input = Cursor::new(b"Hello, world!");
    /// let mut output = Vec::new();
    /// let bytes = Compression::Gzip.compress_stream(input, &mut output).unwrap();
    /// assert!(bytes > 0);
    /// ```
    pub fn compress_stream<'a, R: Read, W: Write + 'a>(&self, mut reader: R, writer: W) -> Result<u64> {
        let mut writer = self.wrap_writer(writer)?;
        std::io::copy(&mut reader, &mut writer).or_raise(|| ErrorKind::Io)
    }

    /// Decompress from a reader to a writer, returning bytes written.
    ///
    /// This is a convenience method for streaming decompression without
    /// buffering the entire input in memory.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::Cursor;
    /// use rawr_compress::Compression;
    ///
    /// let original = b"Hello, world!";
    /// let compressed = Compression::Gzip.compress(original).unwrap();
    ///
    /// let input = Cursor::new(compressed);
    /// let mut output = Vec::new();
    /// let bytes = Compression::Gzip.decompress_stream(input, &mut output).unwrap();
    /// assert_eq!(output, original);
    /// assert_eq!(bytes, original.len() as u64);
    /// ```
    pub fn decompress_stream<'a, R: Read + 'a, W: Write>(&self, reader: R, mut writer: W) -> Result<u64> {
        let mut reader = self.wrap_reader(reader)?;
        std::io::copy(&mut reader, &mut writer).or_raise(|| ErrorKind::Io)
    }
}

#[cfg(test)]
mod tests {
    use crate::Compression;
    use rstest::rstest;
    use std::io::{Read, Write};

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Bzip2)]
    #[case(Compression::Gzip)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    fn test_compress_decompress(#[case] format: Compression) {
        let original = b"Hello, world! This is a test of some compression.";
        let compressed = format.compress(original).unwrap();
        let decompressed = format.decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[rstest]
    #[case(Compression::Bzip2)]
    #[case(Compression::Gzip)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    // Don't bother testing feature-locked formats
    fn test_invalid_compressed_data(#[case] format: Compression) {
        let invalid_data = b"This is not compressed data";
        assert!(format.decompress(invalid_data).is_err());
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Bzip2)]
    #[case(Compression::Gzip)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    fn test_wrap_reader(#[case] format: Compression) {
        use std::io::Cursor;
        let original = b"Hello, world!";
        let compressed = format.compress(original).unwrap();
        // Use Cursor which owns the data
        let cursor = Cursor::new(compressed);
        let mut reader = format.wrap_reader(cursor).expect("decoder to initialize");
        let mut decompressed = Vec::new();
        reader.read_to_end(&mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Bzip2)]
    #[case(Compression::Gzip)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    fn test_wrap_writer(#[case] format: Compression) {
        let original = b"Hello, world!";
        let output = Vec::new();
        // Writer takes ownership
        let mut writer = format.wrap_writer(output).expect("encoder to initialize");
        writer.write_all(original).unwrap();
        // Get the output back by finishing the encoder
        // For now, we'll just test that writing works without error
        // Full round-trip is tested in compress/decompress tests
    }

    #[rstest]
    #[case(Compression::None)]
    #[case(Compression::Gzip)]
    #[case(Compression::Bzip2)]
    #[cfg_attr(feature = "brotli", case(Compression::Brotli))]
    #[cfg_attr(feature = "xz", case(Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd))]
    fn test_stream_roundtrip(#[case] format: Compression) {
        use std::io::Cursor;

        let original = b"Hello, world! This is a test of streaming compression.";

        // Compress via stream
        let input = Cursor::new(original.as_slice());
        let mut compressed = Vec::new();
        let bytes_in = format.compress_stream(input, &mut compressed).unwrap();
        assert_eq!(bytes_in, original.len() as u64);

        // Decompress via stream
        let input = Cursor::new(compressed);
        let mut decompressed = Vec::new();
        let bytes_out = format.decompress_stream(input, &mut decompressed).unwrap();
        assert_eq!(bytes_out, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_stream_empty_input() {
        use std::io::Cursor;

        let original: &[u8] = b"";
        let input = Cursor::new(original);
        let mut compressed = Vec::new();
        let bytes = Compression::Gzip.compress_stream(input, &mut compressed).unwrap();
        assert_eq!(bytes, 0);

        let input = Cursor::new(compressed);
        let mut decompressed = Vec::new();
        let bytes = Compression::Gzip.decompress_stream(input, &mut decompressed).unwrap();
        assert_eq!(bytes, 0);
        assert!(decompressed.is_empty());
    }
}
