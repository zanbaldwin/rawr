use crate::Compression;
use std::fmt::{Display, Formatter, Result as FmtResult};

impl Display for Compression {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

impl AsRef<str> for Compression {
    fn as_ref(&self) -> &'static str {
        self.as_str()
    }
}

impl Compression {
    /// Returns the file extension for this compression format.
    #[inline]
    #[must_use]
    pub fn extension(&self) -> &'static str {
        match self {
            Compression::None => "",
            #[cfg(feature = "brotli")]
            Compression::Brotli => ".br",
            Compression::Bzip2 => ".bz2",
            Compression::Gzip => ".gz",
            #[cfg(feature = "xz")]
            Compression::Xz => ".xz",
            #[cfg(feature = "zstd")]
            Compression::Zstd => ".zst",
        }
    }

    /// Returns the short name for configuration (for displaying to user)
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Compression::None => "none",
            #[cfg(feature = "brotli")]
            Compression::Brotli => "brotli",
            Compression::Bzip2 => "bzip2",
            Compression::Gzip => "gzip",
            #[cfg(feature = "xz")]
            Compression::Xz => "xz",
            #[cfg(feature = "zstd")]
            Compression::Zstd => "zstd",
        }
    }

    /// Double-check the detected compression format (from the path
    /// extension) with the actual contents of the file.
    #[must_use]
    pub fn check_magic_bytes(&self, bytes: &[u8]) -> bool {
        #[cfg(feature = "brotli")]
        if matches!(self, Compression::Brotli) {
            // Brotli doesn't use magic bytes, so we
            // just have to assume that it's correct.
            return true;
        }
        *self == Self::from_magic_bytes(bytes)
    }
}

#[cfg(test)]
mod tests {
    use crate::Compression;
    use rstest::rstest;

    #[rstest]
    #[case(Compression::None, "")]
    #[case(Compression::Bzip2, ".bz2")]
    #[case(Compression::Gzip, ".gz")]
    fn test_extension_default(#[case] format: Compression, #[case] expected: &str) {
        assert_eq!(format.extension(), expected);
    }

    #[rstest]
    #[cfg(feature = "brotli")]
    #[case(Compression::Brotli, ".br")]
    fn test_extension_brotli(#[case] format: Compression, #[case] expected: &str) {
        assert_eq!(format.extension(), expected);
    }

    #[rstest]
    #[cfg(feature = "xz")]
    #[case(Compression::Xz, ".xz")]
    fn test_extension_xz(#[case] format: Compression, #[case] expected: &str) {
        assert_eq!(format.extension(), expected);
    }

    #[rstest]
    #[cfg(feature = "zstd")]
    #[case(Compression::Zstd, ".zst")]
    fn test_extension_zstd(#[case] format: Compression, #[case] expected: &str) {
        assert_eq!(format.extension(), expected);
    }
}
