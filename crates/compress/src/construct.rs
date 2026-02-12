use crate::Compression;
use crate::error::{Error, ErrorKind};
use std::{path::Path, str::FromStr};

const BZIP2_MAGIC: [u8; 3] = [0x42, 0x5A, 0x68];
const GZIP_MAGIC: [u8; 2] = [0x1F, 0x8B];
#[cfg(feature = "xz")]
const XZ_MAGIC: [u8; 6] = [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00];
#[cfg(feature = "zstd")]
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

impl FromStr for Compression {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(Compression::None),
            #[cfg(feature = "brotli")]
            "br" | "brotli" => Ok(Compression::Brotli),
            #[cfg(not(feature = "brotli"))]
            "br" | "brotli" => exn::bail!(ErrorKind::DisabledFormat(s.to_string())),
            "bz2" | "bzip2" => Ok(Compression::Bzip2),
            "gz" | "gzip" => Ok(Compression::Gzip),
            #[cfg(feature = "xz")]
            "xz" | "lzma" => Ok(Compression::Xz),
            #[cfg(not(feature = "xz"))]
            "xz" | "lzma" => exn::bail!(ErrorKind::DisabledFormat(s.to_string())),
            #[cfg(feature = "zstd")]
            "zst" | "zstd" => Ok(Compression::Zstd),
            #[cfg(not(feature = "zstd"))]
            "zst" | "zstd" => exn::bail!(ErrorKind::DisabledFormat(s.to_string())),
            _ => exn::bail!(ErrorKind::UnsupportedFormat(s.to_string())),
        }
    }
}
impl From<&[u8]> for Compression {
    fn from(value: &[u8]) -> Self {
        Compression::from_magic_bytes(value)
    }
}
impl Compression {
    /// Detect compression from a file extension.
    #[must_use]
    pub fn from_path(path: impl AsRef<Path>) -> Self {
        path.as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| match ext.to_lowercase().as_str() {
                #[cfg(feature = "brotli")]
                "br" => Compression::Brotli,
                "bz2" => Compression::Bzip2,
                "gz" => Compression::Gzip,
                #[cfg(feature = "xz")]
                "xz" => Compression::Xz,
                #[cfg(feature = "zstd")]
                "zst" => Compression::Zstd,
                _ => Compression::None,
            })
            .unwrap_or(Compression::None)
    }

    /// Detect compression format from magic bytes.
    ///
    /// Returns `None` variant if no magic bytes match or if the input
    /// is too short to detect any format.
    #[must_use]
    pub fn from_magic_bytes(bytes: &[u8]) -> Self {
        // Brotli does not have standardized magic bytes (uses container formats).
        if bytes.starts_with(&BZIP2_MAGIC) {
            return Compression::Bzip2;
        }
        if bytes.starts_with(&GZIP_MAGIC) {
            return Compression::Gzip;
        }
        #[cfg(feature = "xz")]
        if bytes.starts_with(&XZ_MAGIC) {
            return Compression::Xz;
        }
        #[cfg(feature = "zstd")]
        if bytes.starts_with(&ZSTD_MAGIC) {
            return Compression::Zstd;
        }
        Compression::None
    }
}

#[cfg(test)]
mod tests {
    use crate::Compression;
    use rstest::rstest;

    #[rstest]
    #[case("none", Compression::None)]
    #[case("bz2", Compression::Bzip2)]
    #[case("bzip2", Compression::Bzip2)]
    #[case("BZIP2", Compression::Bzip2)]
    #[case("gz", Compression::Gzip)]
    #[case("gzip", Compression::Gzip)]
    #[cfg_attr(feature = "brotli", case("br", Compression::Brotli))]
    #[cfg_attr(feature = "brotli", case("br", Compression::Brotli))]
    #[cfg_attr(feature = "xz", case("xz", Compression::Xz))]
    #[cfg_attr(feature = "xz", case("lzma", Compression::Xz))]
    #[cfg_attr(feature = "zstd", case("zst", Compression::Zstd))]
    #[cfg_attr(feature = "zstd", case("zstd", Compression::Zstd))]
    fn test_from_str(#[case] test: &str, #[case] expected: Compression) {
        assert_eq!(test.parse::<Compression>().unwrap(), expected);
    }

    #[rstest]
    #[case("invalid")]
    #[case("definitely not valid")]
    #[case(" ")]
    fn test_from_str_invalid(#[case] test: &str) {
        assert!(test.parse::<Compression>().is_err());
    }

    #[rstest]
    #[case("file.html", Compression::None)]
    #[case("file.txt", Compression::None)]
    // `.bz2` is a dotfile with no extension (like `.bashrc`), and therefore
    // with no extension is considered to have no compression.
    #[case(".bz2", Compression::None)]
    #[case("file.html.bz2", Compression::Bzip2)]
    #[case("file.html.gz", Compression::Gzip)]
    #[case("file.gz", Compression::Gzip)]
    #[cfg_attr(feature = "brotli", case("file.html.br", Compression::Brotli))]
    #[cfg_attr(feature = "xz", case("file.html.xz", Compression::Xz))]
    #[cfg_attr(feature = "zstd", case("file.html.zst", Compression::Zstd))]
    fn test_from_path_default(#[case] test: &str, #[case] expected: Compression) {
        assert_eq!(Compression::from_path(test), expected);
    }

    #[rstest]
    #[case(b"<!DOCTYPE html>", Compression::None)]
    #[case(b"", Compression::None)]
    #[case(&[], Compression::None)]
    #[case(&[0x42, 0x5A, 0x68, 0x39], Compression::Bzip2)]
    #[case(&[0x1F, 0x8B, 0x08, 0x00], Compression::Gzip)]
    #[cfg_attr(feature = "xz", case(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0x00], Compression::Xz))]
    #[cfg_attr(feature = "zstd", case(&[0x28, 0xB5, 0x2F, 0xFD], Compression::Zstd))]
    fn test_from_magic_bytes_default(#[case] bytes: &[u8], #[case] expected: Compression) {
        assert_eq!(Compression::from_magic_bytes(bytes), expected);
        assert_eq!(<&[u8] as Into<Compression>>::into(bytes), expected);
    }
}
