//! Path validation and security utilities.
//!
//! This module provides functions to validate storage paths and prevent
//! security issues like path traversal attacks.

use std::path::{Component, Path, PathBuf};

use crate::error::{ErrorKind, Result};

/// Validates a storage path for security and correctness.
/// Ensures that paths don't escape the storage root (no `..` traversal).
///
/// > **Note:** This does **not** normalize backslashes, non-UTF8 bytes, or
/// >           platform-specific weirdness. Null bytes are explicitly rejected.
///
/// # Returns
/// Returns the normalized path if valid, or [`InvalidPath`](crate::error::ErrorKind::InvalidPath)
/// if invalid.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use rawr_storage::validate_path;
/// // Valid paths
/// assert!(validate_path("Fandom/work.html.bz2").is_ok());
/// assert!(validate_path("a/b/c/file.html").is_ok());
/// assert!(validate_path("a/../file.html").is_ok()); // (never leaves library root)
/// // Invalid paths
/// assert!(validate_path("../etc/passwd").is_err());
/// assert!(validate_path("a/../../b").is_err()); // (leaves library root)
/// assert!(validate_path("a\0b").is_err());
/// // Paths get resolved
/// assert_eq!(
///     validate_path("wrong/../still-wrong/.././correct//./path.html/").unwrap(),
///     Path::new("correct/path.html")
/// );
/// ```
pub fn validate(path: impl AsRef<Path>) -> Result<PathBuf> {
    // Use Rust's built-in path component parser for robust handling. Means we
    // don't have to deal with non-UTF8, or the maniacs on Unix that use
    // backslashes in their filenames.
    let mut components = Vec::new();
    for component in path.as_ref().components() {
        match component {
            Component::Normal(s) => {
                // Null bytes pass through Path::components() on Unix but cause
                // truncation in C-based syscalls — reject them explicitly.
                if s.as_encoded_bytes().contains(&0) {
                    exn::bail!(ErrorKind::InvalidPath(path.as_ref().to_path_buf()));
                }
                components.push(s)
            },
            Component::CurDir | Component::RootDir => {},
            // Yeah, fuck off Windows.
            Component::Prefix(_) => exn::bail!(ErrorKind::InvalidPath(path.as_ref().to_path_buf())),
            Component::ParentDir => {
                if components.pop().is_none() {
                    exn::bail!(ErrorKind::InvalidPath(path.as_ref().to_path_buf()));
                }
            },
        }
    }
    match components.is_empty() {
        true => exn::bail!(ErrorKind::InvalidPath(path.as_ref().to_path_buf())),
        false => Ok(components.into_iter().collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_paths() {
        assert_eq!(validate(Path::new("Fandom/work.html.bz2")).unwrap(), Path::new("Fandom/work.html.bz2"));
        assert_eq!(validate(Path::new("a/b/c/file.html")).unwrap(), Path::new("a/b/c/file.html"));
        assert_eq!(validate(Path::new("simple.html")).unwrap(), Path::new("simple.html"));
    }

    #[test]
    fn test_path_normalization() {
        // Double slashes are normalized
        assert_eq!(validate(Path::new("a//b//c")).unwrap(), Path::new("a/b/c"));
        // Current directory references removed
        assert_eq!(validate(Path::new("a/./b/./c")).unwrap(), Path::new("a/b/c"));
    }

    #[cfg(windows)]
    #[test]
    fn test_backslash_normalization() {
        // On Windows, backslashes are path separators and get normalized
        assert_eq!(validate(Path::new("a\\b\\c")).unwrap(), Path::new("a/b/c"));
        assert_eq!(validate(Path::new("a\\b/c\\d")).unwrap(), Path::new("a/b/c/d"));
    }

    #[test]
    fn test_traversal_attempts() {
        // Basic parent directory reference
        assert!(validate(Path::new("../etc/passwd")).is_err());
        // Traversal in the middle
        assert!(validate(Path::new("a/../../b")).is_err());
        // Only parent references
        assert!(validate(Path::new("..")).is_err());
        assert!(validate(Path::new("../..")).is_err());
    }

    #[test]
    fn test_reverse_attempts() {
        // Traversal remains within library root
        assert_eq!(validate(Path::new("a/b/..")).unwrap(), Path::new("a"));
    }

    #[test]
    fn test_invalid_characters() {
        // Null byte
        assert!(validate(Path::new("a\0b")).is_err());
        assert!(validate(Path::new("\0")).is_err());
    }

    #[test]
    fn test_empty_paths() {
        // Empty string
        assert!(validate(Path::new("")).is_err());
        // Only dots and slashes (normalizes to empty)
        assert!(validate(Path::new(".")).is_err());
        assert!(validate(Path::new("./")).is_err());
        assert!(validate(Path::new("./.")).is_err());
        assert!(validate(Path::new("//")).is_err());
    }

    #[test]
    fn test_trailing_slashes() {
        // Trailing slashes should be stripped
        assert_eq!(validate(Path::new("Fandom1/")).unwrap(), Path::new("Fandom1"));
        assert_eq!(validate(Path::new("a/b/c/")).unwrap(), Path::new("a/b/c"));
        assert_eq!(validate(Path::new("file.html/")).unwrap(), Path::new("file.html"));
        // Multiple trailing slashes
        assert_eq!(validate(Path::new("Fandom1///")).unwrap(), Path::new("Fandom1"));
    }
}
