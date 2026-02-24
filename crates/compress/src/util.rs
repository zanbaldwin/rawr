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

    /// Verify that `bytes` start with the expected magic bytes for this format.
    ///
    /// Useful for cross-checking a format detected from a file extension against
    /// actual file contents. Returns `true` for Brotli unconditionally,
    /// since Brotli has no standardized magic bytes.
    #[must_use]
    pub fn check_magic_bytes(&self, bytes: &[u8]) -> bool {
        #[cfg(feature = "brotli")]
        if matches!(self, Compression::Brotli) {
            // Brotli doesn't use magic bytes, so we
            // just have to assume that it's correct.
            return true;
        }
        match Self::from_magic_bytes(bytes) {
            Some(f) => *self == f,
            None => matches!(self, Compression::None),
        }
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
    #[cfg_attr(feature = "brotli", case(Compression::Brotli, ".br"))]
    #[cfg_attr(feature = "xz", case(Compression::Xz, ".xz"))]
    #[cfg_attr(feature = "zstd", case(Compression::Zstd, ".zst"))]
    fn test_extension_default(#[case] format: Compression, #[case] expected: &str) {
        assert_eq!(format.extension(), expected);
    }
}
