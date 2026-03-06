//! Path validation and security utilities.
//!
//! This module provides functions to validate storage paths and prevent
//! security issues like path traversal attacks.

// This will be used soon, as OpenDAL operates purely on strings. No point
// keeping using potentially non-UTF8 Path(Buf)s when we have to convert to
// UTF8 strings anyway.

use crate::error::{Error, ErrorKind, Result};
use exn::OptionExt;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::ops::Deref;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

// Internal boundary-conversation helper trait. Probably shouldn't
// use this, and just be specific with my conversions.
pub(crate) trait TryValidatePath: TryInto<ValidatedPath, Error = Error> {}
impl<T: TryInto<ValidatedPath, Error = Error>> TryValidatePath for T {}

/// Validates a storage path for security and correctness.
/// Ensures that paths don't escape the storage root (no `..` traversal).
///
/// > **Note:** This does **not** normalize backslashes, or other platform-specific
/// > weirdness. Null bytes are explicitly rejected.
///
/// # Returns
/// Returns the normalized path if valid, or [`InvalidPath`](crate::error::ErrorKind::InvalidPath)
/// if invalid.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use rawr_storage::ValidatedPath;
/// // Valid paths
/// assert!(ValidatedPath::new("Fandom/work.html.bz2").is_ok());
/// assert!(ValidatedPath::new("a/b/c/file.html").is_ok());
/// assert!(ValidatedPath::new("a/../file.html").is_ok()); // (never leaves library root)
/// // Invalid paths
/// assert!(ValidatedPath::new("../etc/passwd").is_err());
/// assert!(ValidatedPath::new("a/../../b").is_err()); // (leaves library root)
/// assert!(ValidatedPath::new("a\0b").is_err());
/// // Paths get resolved
/// assert_eq!(
///     ValidatedPath::new("wrong/../still-wrong/.././correct//./path.html/").unwrap(),
///     "correct/path.html"
/// );
/// ```
#[derive(Clone, Debug)]
pub struct ValidatedPath(String);
impl Deref for ValidatedPath {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Display for ValidatedPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.0)
    }
}
impl ValidatedPath {
    pub fn new(value: impl AsRef<Path>) -> Result<Self> {
        let path = value.as_ref();
        // Use Rust's built-in path component parser for robust handling. Means we
        // don't have to deal with non-UTF8, or the maniacs on Unix that use
        // backslashes in their filenames.
        let mut components = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(s) => {
                    // Null bytes pass through Path::components() on Unix but cause
                    // truncation in C-based syscalls — reject them explicitly.
                    if s.as_encoded_bytes().contains(&0) {
                        exn::bail!(ErrorKind::InvalidPath(path.to_path_buf()));
                    }
                    components.push(s.to_str().ok_or_raise(|| ErrorKind::InvalidPath(path.to_path_buf()))?)
                },
                Component::CurDir | Component::RootDir => {},
                // Yeah, fuck off Windows.
                Component::Prefix(_) => exn::bail!(ErrorKind::InvalidPath(path.to_path_buf())),
                Component::ParentDir => {
                    if components.pop().is_none() {
                        exn::bail!(ErrorKind::InvalidPath(path.to_path_buf()));
                    }
                },
            }
        }
        if components.is_empty() {
            exn::bail!(ErrorKind::InvalidPath(path.to_path_buf()));
        }
        Ok(Self(components.join("/")))
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self.deref())
    }
}
impl AsRef<Path> for ValidatedPath {
    fn as_ref(&self) -> &Path {
        Path::new(self.deref())
    }
}
impl AsRef<str> for ValidatedPath {
    fn as_ref(&self) -> &str {
        self.deref()
    }
}
impl TryFrom<&Path> for ValidatedPath {
    type Error = Error;
    fn try_from(value: &Path) -> std::result::Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl TryFrom<PathBuf> for ValidatedPath {
    type Error = Error;
    fn try_from(value: PathBuf) -> std::result::Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl TryFrom<&str> for ValidatedPath {
    type Error = Error;
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl TryFrom<String> for ValidatedPath {
    type Error = Error;
    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl FromStr for ValidatedPath {
    type Err = Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::new(s)
    }
}
impl From<ValidatedPath> for String {
    fn from(value: ValidatedPath) -> Self {
        value.0
    }
}
impl From<ValidatedPath> for PathBuf {
    fn from(value: ValidatedPath) -> Self {
        PathBuf::from(value.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_paths() {
        assert_eq!(*ValidatedPath::new("Fandom/work.html.bz2").unwrap(), "Fandom/work.html.bz2");
        assert_eq!(*ValidatedPath::new("a/b/c/file.html").unwrap(), "a/b/c/file.html");
        assert_eq!(*ValidatedPath::new("simple.html").unwrap(), "simple.html");
    }

    #[test]
    fn test_path_normalization() {
        // Double slashes are normalized
        assert_eq!(*ValidatedPath::new("a//b//c").unwrap(), "a/b/c");
        // Current directory references removed
        assert_eq!(*ValidatedPath::new("a/./b/./c").unwrap(), "a/b/c");
    }

    #[cfg(windows)]
    #[test]
    fn test_backslash_normalization() {
        // On Windows, backslashes are path separators and get normalized
        assert_eq!(*ValidatePath::new("a\\b\\c").unwrap(), "a/b/c");
        assert_eq!(*ValidatePath::new("a\\b/c\\d").unwrap(), "a/b/c/d");
    }

    #[test]
    fn test_traversal_attempts() {
        // Basic parent directory reference
        assert!(ValidatedPath::new("../etc/passwd").is_err());
        // Traversal in the middle
        assert!(ValidatedPath::new("a/../../b").is_err());
        // Only parent references
        assert!(ValidatedPath::new("..").is_err());
        assert!(ValidatedPath::new("../..").is_err());
    }

    #[test]
    fn test_reverse_attempts() {
        // Traversal remains within library root
        assert_eq!(*ValidatedPath::new("a/b/..").unwrap(), "a");
    }

    #[test]
    fn test_invalid_characters() {
        // Null byte
        assert!(ValidatedPath::new("a\0b").is_err());
        assert!(ValidatedPath::new("\0").is_err());
    }

    #[test]
    fn test_empty_paths() {
        // Empty string
        assert!(ValidatedPath::new("").is_err());
        // Only dots and slashes (normalizes to empty)
        assert!(ValidatedPath::new(".").is_err());
        assert!(ValidatedPath::new("./").is_err());
        assert!(ValidatedPath::new("./.").is_err());
        assert!(ValidatedPath::new("//").is_err());
    }

    #[test]
    fn test_trailing_slashes() {
        // Trailing slashes should be stripped
        assert_eq!(*ValidatedPath::new("Fandom1/").unwrap(), "Fandom1");
        assert_eq!(*ValidatedPath::new("a/b/c/").unwrap(), "a/b/c");
        assert_eq!(*ValidatedPath::new("file.html/").unwrap(), "file.html");
        // Multiple trailing slashes
        assert_eq!(*ValidatedPath::new("Fandom1///").unwrap(), "Fandom1");
    }
}
