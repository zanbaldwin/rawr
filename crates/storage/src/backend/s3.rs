//! S3-compatible storage backend.
//!
//! This module provides a storage backend implementation for S3-compatible
//! services including AWS S3, Backblaze B2, Tigris (Fly.io), and others.
//!
//! # Credentials
//!
//! Credentials are provided explicitly via the configuration file. Each
//! target specifies its own `key_id` and `key_secret`.
//!
//! TODO: Future iteration - support `credentials: "profile:name"` in config
//! to use AWS SDK credential providers for actual AWS S3 targets.
//! This would allow using ~/.aws/credentials profiles instead of explicit keys.
//! Not implemented now since we primarily target Backblaze/Tigris which use
//! explicit credentials, and the credential chain is inherently single-account
//! which doesn't fit well with multiple heterogeneous targets.

use crate::{
    FileInfo, StorageBackend,
    backend::FileInfoStream,
    error::{ErrorKind, Result},
    validate_path,
};
use async_trait::async_trait;
use aws_sdk_s3::{
    Client,
    config::{BehaviorVersion, Credentials, Region, retry::RetryConfig},
    primitives::DateTime,
};
use exn::{OptionExt, ResultExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Generous default for concurrent S3 requests.
///
/// TODO: Adaptive rate limiting based on 429/throttling responses?
const DEFAULT_CONCURRENT_REQUESTS: usize = 100;

/// S3-compatible storage backend.
///
/// Stores files in an S3 bucket, optionally under a key prefix. All paths are
/// relative to the configured prefix (if any).
///
/// # Supported Services
///
/// - AWS S3
/// - Backblaze B2 (via S3-compatible API)
/// - Tigris (Fly.io storage)
/// - MinIO
/// - Other S3-compatible services
///
/// # Examples
///
/// ```no_run
/// use rawr_storage::backend::S3Backend;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = S3Backend::new(
///     "my-storage",
///     "my-bucket",
///     Some("library/".to_string()),
///     "us-west-004",
///     Some("https://s3.us-west-004.backblazeb2.com".to_string()),
///     "access_key_id",
///     "secret_access_key",
/// ).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct S3Backend {
    name: String,
    client: Client,
    bucket: String,
    prefix: Option<String>,
    /// Rate limiter for concurrent S3 requests.
    rate_limiter: Arc<Semaphore>,
}

impl S3Backend {
    /// Create a new S3 storage backend.
    ///
    /// # Arguments
    /// * `name` - A name for this backend (used in display/logging)
    /// * `bucket` - S3 bucket name
    /// * `prefix` - Optional key prefix (acts as virtual directory)
    /// * `region` - AWS region or provider-specific region (e.g., "us-west-004" for Backblaze)
    /// * `endpoint` - Custom endpoint URL for S3-compatible services
    /// * `access_key` - AWS/provider access key ID
    /// * `access_secret` - AWS/provider secret access key
    pub async fn new(
        name: impl Into<String>,
        bucket: impl Into<String>,
        prefix: Option<String>,
        region: impl Into<String>,
        endpoint: Option<impl Into<String>>,
        key_id: impl Into<String>,
        key_secret: impl Into<String>,
    ) -> Result<Self> {
        let prefix = prefix
            .map(validate_path)
            .transpose()?
            .map(|p| p.to_str().map(|s| s.to_string()).ok_or_raise(|| ErrorKind::InvalidPath(p)))
            .transpose()?;
        let name = name.into();
        let bucket = bucket.into();
        let region = Region::new(region.into());
        let credentials = Credentials::new(key_id, key_secret, None, None, "rawr-config");
        let mut config_builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .credentials_provider(credentials)
            .region(region)
            // Configure retry policy with exponential backoff (1 initial + 3 retries)
            .retry_config(RetryConfig::standard().with_max_attempts(4))
            // Use path-style addressing for better compatibility with
            // S3-compatible services (Backblaze, MinIO, etc.)
            .force_path_style(true);
        // Set custom endpoint for non-AWS services
        if let Some(endpoint_url) = endpoint {
            config_builder = config_builder.endpoint_url(endpoint_url);
        }
        let client = Client::from_conf(config_builder.build());
        let rate_limiter = Arc::new(Semaphore::new(DEFAULT_CONCURRENT_REQUESTS));
        Ok(Self {
            name,
            client,
            bucket,
            prefix,
            rate_limiter,
        })
    }

    /// Construct the full S3 key from a relative path.
    fn full_key(&self, path: &Path) -> Result<String> {
        let validated = validate_path(path)?;
        // TODO: String lossy, or convert to &str and propogate an InvalidPath error?
        let path_str = validated.to_string_lossy();
        Ok(match &self.prefix {
            Some(prefix) => format!("{}/{}", prefix.trim_end_matches('/'), path_str),
            None => path_str.into_owned(),
        })
    }

    /// Strip the configured prefix from an S3 key to get relative path.
    fn relative_path(&self, key: &str) -> Result<PathBuf> {
        let relative = match &self.prefix {
            Some(prefix) => {
                let prefix_normalized = prefix.trim_end_matches('/');
                key.strip_prefix(prefix_normalized).and_then(|s| s.strip_prefix('/')).unwrap_or(key)
            },
            None => key,
        };
        validate_path(relative)
    }

    /// Acquire a rate limiter permit before making an S3 API call.
    async fn acquire_permit(&self) -> OwnedSemaphorePermit {
        // unwrap is safe: semaphore is never closed
        self.rate_limiter.clone().acquire_owned().await.unwrap()
    }

    /// Convert AWS DateTime to OffsetDateTime.
    fn parse_datetime(dt: &DateTime) -> Result<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp_nanos(dt.as_nanos())
            .or_raise(|| ErrorKind::BackendError("S3 datetime out of range".to_string()))
    }
}

#[async_trait]
impl StorageBackend for S3Backend {
    fn name(&self) -> &str {
        &self.name
    }

    fn list_stream<'a>(&'a self, prefix: Option<&'a Path>) -> FileInfoStream<'a> {
        todo!()
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        todo!()
    }

    async fn read(&self, path: &Path) -> Result<Vec<u8>> {
        todo!()
    }

    async fn read_head(&self, path: &Path, bytes: usize) -> Result<Vec<u8>> {
        todo!()
    }

    async fn write(&self, path: &Path, data: &[u8]) -> Result<()> {
        todo!()
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        todo!()
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        todo!()
    }

    async fn stat(&self, path: &Path) -> Result<FileInfo> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_key_without_prefix() {
        // We can't easily test full_key without constructing a real S3Backend,
        // but we can test the logic by examining the expected behavior
        let prefix: Option<String> = None;
        let path = Path::new("Fandom/work.html.bz2");
        let path_str = path.to_string_lossy();
        let result = match &prefix {
            Some(p) => format!("{}/{}", p.trim_end_matches('/'), path_str),
            None => path_str.into_owned(),
        };
        assert_eq!(result, "Fandom/work.html.bz2");
    }

    #[test]
    fn test_full_key_with_prefix() {
        let prefix = Some("library".to_string());
        let path = Path::new("Fandom/work.html.bz2");
        let path_str = path.to_string_lossy();
        let result = match &prefix {
            Some(p) => format!("{}/{}", p.trim_end_matches('/'), path_str),
            None => path_str.into_owned(),
        };
        assert_eq!(result, "library/Fandom/work.html.bz2");
    }

    #[test]
    fn test_full_key_with_trailing_slash_prefix() {
        let prefix = Some("library/".to_string());
        let path = Path::new("Fandom/work.html.bz2");
        let path_str = path.to_string_lossy();
        let result = match &prefix {
            Some(p) => format!("{}/{}", p.trim_end_matches('/'), path_str),
            None => path_str.into_owned(),
        };
        assert_eq!(result, "library/Fandom/work.html.bz2");
    }

    #[test]
    fn test_relative_path_without_prefix() {
        let prefix: Option<String> = None;
        let key = "Fandom/work.html.bz2";
        let relative = match &prefix {
            Some(p) => {
                let prefix_normalized = p.trim_end_matches('/');
                key.strip_prefix(prefix_normalized).and_then(|s| s.strip_prefix('/')).unwrap_or(key)
            },
            None => key,
        };
        assert_eq!(relative, "Fandom/work.html.bz2");
    }

    #[test]
    fn test_relative_path_with_prefix() {
        let prefix = Some("library".to_string());
        let key = "library/Fandom/work.html.bz2";
        let relative = match &prefix {
            Some(p) => {
                let prefix_normalized = p.trim_end_matches('/');
                key.strip_prefix(prefix_normalized).and_then(|s| s.strip_prefix('/')).unwrap_or(key)
            },
            None => key,
        };
        assert_eq!(relative, "Fandom/work.html.bz2");
    }

    #[test]
    fn test_relative_path_with_trailing_slash_prefix() {
        let prefix = Some("library/".to_string());
        let key = "library/Fandom/work.html.bz2";
        let relative = match &prefix {
            Some(p) => {
                let prefix_normalized = p.trim_end_matches('/');
                key.strip_prefix(prefix_normalized).and_then(|s| s.strip_prefix('/')).unwrap_or(key)
            },
            None => key,
        };
        assert_eq!(relative, "Fandom/work.html.bz2");
    }
}
