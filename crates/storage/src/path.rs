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
#[derive(Clone, Debug, PartialEq, Eq)]
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
                        exn::bail!(ErrorKind::InvalidPath(path.display().to_string()));
                    }
                    components.push(s.to_str().ok_or_raise(|| ErrorKind::InvalidPath(path.display().to_string()))?)
                },
                Component::CurDir | Component::RootDir => {},
                // Yeah, fuck off Windows.
                Component::Prefix(_) => exn::bail!(ErrorKind::InvalidPath(path.display().to_string())),
                Component::ParentDir => {
                    if components.pop().is_none() {
                        exn::bail!(ErrorKind::InvalidPath(path.display().to_string()));
                    }
                },
            }
        }
        if components.is_empty() {
            exn::bail!(ErrorKind::InvalidPath(path.display().to_string()));
        }
        Ok(Self(components.join("/")))
    }

    /// Reference to Validated Path as `std` Path
    ///
    /// Can't implement `AsRef<Path>` for ValidatedPath (breaks the blanket
    /// impl), so this is the compromise.
    pub fn as_path(&self) -> &Path {
        Path::new(self.deref())
    }
}

pub trait TryValidatePath {
    fn try_validate(self) -> Result<ValidatedPath>;
}
impl<T: AsRef<Path>> TryValidatePath for T {
    fn try_validate(self) -> Result<ValidatedPath> {
        ValidatedPath::new(self)
    }
}
/// Zero-allocation passing of already validated paths.
impl TryValidatePath for ValidatedPath {
    fn try_validate(self) -> Result<ValidatedPath> {
        Ok(self)
    }
}
impl TryValidatePath for &ValidatedPath {
    fn try_validate(self) -> Result<ValidatedPath> {
        Ok(self.clone())
    }
}
impl FromStr for ValidatedPath {
    type Err = Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        s.try_validate()
    }
}
// TODO: impl<T> TryFrom<T> for ValidatedPath where T: TryValidatePath {}
//       But it would require to not implement TryValidatePath for ValidatedPath,
//       which I want for zero-allocation passing of already validated paths.

impl AsRef<str> for ValidatedPath {
    fn as_ref(&self) -> &str {
        self.deref().as_ref()
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

impl PartialEq<str> for ValidatedPath {
    fn eq(&self, other: &str) -> bool {
        self.deref() == other
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
