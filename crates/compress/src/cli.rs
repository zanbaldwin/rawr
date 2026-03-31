//! CLI helpers for resolving compression from command-line flags.
//!
//! Maps the `--compress=FORMAT` / `--no-compress` / omitted pattern
//! into a [`Preference`] that can be resolved against a configured
//! default and an original file's format.

use crate::Compression;
use crate::error::Error;
use std::str::FromStr;

/// Resolved user intent for compression from a CLI invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Preference {
    /// An explicit format was specified via `--compress=FORMAT`
    Explicit(Compression),
    /// No compression flag was provided — use the configured default
    Implicit,
    /// Compression was explicitly disabled via `--no-compress`
    NotSpecified,
}

impl Preference {
    /// Build a [`Preference`] from the `--compress` / `--no-compress` flag pair.
    pub fn from_flags(compress: Option<String>, no_compress: bool) -> Result<Self, Error> {
        if no_compress {
            return Ok(Self::NotSpecified);
        }
        match compress {
            Some(s) => Ok(Self::Explicit(Compression::from_str(&s)?)),
            None => Ok(Self::Implicit),
        }
    }
}

impl Preference {
    /// Determine the final [`Compression`] format to use.
    ///
    /// Resolution order:
    /// - [`Explicit`](Self::Explicit): use the user-specified format
    /// - [`Implicit`](Self::Implicit): use `configured` (the application default)
    /// - [`NotSpecified`](Self::NotSpecified): preserve `original` (the source
    ///   file's format), falling back to [`Compression::None`]
    pub fn resolve(&self, configured: Compression, original: Option<Compression>) -> Compression {
        match self {
            Self::Explicit(c) => *c,
            Self::Implicit => configured,
            Self::NotSpecified => original.unwrap_or(Compression::None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(None, false, Preference::Implicit)]
    #[case(None, true, Preference::NotSpecified)]
    #[case(Some("gz".to_string()), false, Preference::Explicit(Compression::Gzip))]
    #[case(Some("gzip".to_string()), false, Preference::Explicit(Compression::Gzip))]
    #[case(Some("bz2".to_string()), false, Preference::Explicit(Compression::Bzip2))]
    #[case(Some("bzip2".to_string()), false, Preference::Explicit(Compression::Bzip2))]
    #[cfg_attr(feature = "brotli", case(Some("br".to_string()), false, Preference::Explicit(Compression::Brotli)))]
    #[cfg_attr(feature = "brotli", case(Some("brotli".to_string()), false, Preference::Explicit(Compression::Brotli)))]
    #[cfg_attr(feature = "bzip3", case(Some("bz3".to_string()), false, Preference::Explicit(Compression::Bzip3)))]
    #[cfg_attr(feature = "bzip3", case(Some("bzip3".to_string()), false, Preference::Explicit(Compression::Bzip3)))]
    #[cfg_attr(feature = "xz", case(Some("xz".to_string()), false, Preference::Explicit(Compression::Xz)))]
    #[cfg_attr(feature = "xz", case(Some("lzma".to_string()), false, Preference::Explicit(Compression::Xz)))]
    #[cfg_attr(feature = "zstd", case(Some("zst".to_string()), false, Preference::Explicit(Compression::Zstd)))]
    #[cfg_attr(feature = "zstd", case(Some("zstd".to_string()), false, Preference::Explicit(Compression::Zstd)))]
    fn test_from_flags(#[case] compress: Option<String>, #[case] no_compress: bool, #[case] expected: Preference) {
        assert_eq!(Preference::from_flags(compress, no_compress).unwrap(), expected);
    }

    #[test]
    fn test_from_flags_invalid() {
        assert!(Preference::from_flags(Some("definitely not valid".to_string()), false).is_err());
    }

    #[rstest]
    #[case(
        Preference::Explicit(Compression::None),
        Compression::Bzip2,
        Some(Compression::None),
        Compression::None
    )]
    #[case(
        Preference::Explicit(Compression::Gzip),
        Compression::Bzip2,
        Some(Compression::None),
        Compression::Gzip
    )]
    #[case(
        Preference::Explicit(Compression::Bzip2),
        Compression::Bzip2,
        Some(Compression::None),
        Compression::Bzip2
    )]
    #[case(Preference::Explicit(Compression::Gzip), Compression::Bzip2, None, Compression::Gzip)]
    #[case(
        Preference::Implicit,
        Compression::Bzip2,
        Some(Compression::None),
        Compression::Bzip2
    )]
    #[case(Preference::Implicit, Compression::None, Some(Compression::Gzip), Compression::None)]
    #[case(
        Preference::NotSpecified,
        Compression::Bzip2,
        Some(Compression::None),
        Compression::None
    )]
    #[case(
        Preference::NotSpecified,
        Compression::Bzip2,
        Some(Compression::Gzip),
        Compression::Gzip
    )]
    #[case(Preference::NotSpecified, Compression::Bzip2, None, Compression::None)]
    fn test_resolve(
        #[case] preference: Preference,
        #[case] config: Compression,
        #[case] source: Option<Compression>,
        #[case] expected: Compression,
    ) {
        assert_eq!(preference.resolve(config, source), expected);
    }
}
