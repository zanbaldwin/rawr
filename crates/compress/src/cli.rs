//! Compression CLI Helpers

use crate::Compression;
use crate::error::Error;
use std::str::FromStr;

pub type Flag = Option<Option<String>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Preference {
    /// Compression format was specified on the command-line
    Explicit(Compression),
    /// Compression flag was enabled on the command-line, but no format was specified
    Implicit,
    /// Compression was omitted from the command-line
    NotSpecified,
}
impl TryFrom<Flag> for Preference {
    type Error = Error;
    fn try_from(value: Flag) -> Result<Self, Self::Error> {
        match value {
            Some(Some(s)) if s.is_empty() => Ok(Self::Implicit),
            Some(Some(s)) => Ok(Self::Explicit(Compression::from_str(&s)?)),
            Some(None) => Ok(Self::Implicit),
            None => Ok(Self::NotSpecified),
        }
    }
}
impl Preference {
    pub fn resolve(&self, configured: &Compression, original: Option<&Compression>) -> Compression {
        match self {
            Self::Explicit(c) => *c,
            Self::Implicit => *configured,
            Self::NotSpecified => original.copied().unwrap_or(Compression::None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(None, Preference::NotSpecified)]
    #[case(Some(None), Preference::Implicit)]
    #[case(Some(Some("gz".to_string())), Preference::Explicit(Compression::Gzip))]
    #[case(Some(Some("gzip".to_string())), Preference::Explicit(Compression::Gzip))]
    #[case(Some(Some("bz2".to_string())), Preference::Explicit(Compression::Bzip2))]
    #[case(Some(Some("bzip2".to_string())), Preference::Explicit(Compression::Bzip2))]
    #[cfg_attr(feature = "brotli", case(Some(Some("br".to_string())), Preference::Explicit(Compression::Brotli)))]
    #[cfg_attr(feature = "brotli", case(Some(Some("brotli".to_string())), Preference::Explicit(Compression::Brotli)))]
    #[cfg_attr(feature = "xz", case(Some(Some("xz".to_string())), Preference::Explicit(Compression::Xz)))]
    #[cfg_attr(feature = "xz", case(Some(Some("lzma".to_string())), Preference::Explicit(Compression::Xz)))]
    #[cfg_attr(feature = "zstd", case(Some(Some("zst".to_string())), Preference::Explicit(Compression::Zstd)))]
    #[cfg_attr(feature = "zstd", case(Some(Some("zstd".to_string())), Preference::Explicit(Compression::Zstd)))]
    // Omitting feature-dependent format XZ, Zstd
    fn test_construct(#[case] flag: Flag, #[case] expected: Preference) {
        let preference: Preference = flag.try_into().unwrap();
        assert_eq!(preference, expected);
    }

    #[test]
    fn test_construct_invalid() {
        let flag = Some(Some("definitely not valid".to_string()));
        let preference: Result<Preference, Error> = flag.try_into();
        assert!(preference.is_err());
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
        assert_eq!(preference.resolve(&config, source.as_ref()), expected);
    }
}
