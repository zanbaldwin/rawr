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
    error::{ProvideErrorMetadata, SdkError},
    operation::{copy_object::CopyObjectError, get_object::GetObjectError, head_object::HeadObjectError},
    primitives::{ByteStream, DateTime},
};
use exn::{OptionExt, ResultExt};
use rawr_compress::Compression;
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
        // TODO: String lossy, or convert to &str and propagate an InvalidPath error?
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
        let key = self.full_key(path)?;
        let _permit = self.acquire_permit().await;
        match self.client.head_object().bucket(&self.bucket).key(&key).send().await {
            Ok(_) => Ok(true),
            Err(SdkError::ServiceError(e)) if matches!(e.err(), HeadObjectError::NotFound(_)) => Ok(false),
            Err(e) => Err(map_head_error(e, path).into()),
        }
    }

    async fn read(&self, path: &Path) -> Result<Vec<u8>> {
        let key = self.full_key(path)?;
        let _permit = self.acquire_permit().await;
        // TODO: Future iteration - implement streaming reads for large files
        //       to reduce memory usage. Current implementation loads entire
        //       file into memory, which is fine for compressed HTML files but
        //       may need optimization for larger content.
        let response =
            self.client.get_object().bucket(&self.bucket).key(&key).send().await.map_err(|e| map_get_error(e, path))?;
        let bytes = response
            .body
            .collect()
            .await
            .or_raise(|| ErrorKind::Network("failed to read response body".to_string()))?
            .into_bytes();
        Ok(bytes.to_vec())
    }

    async fn read_head(&self, path: &Path, bytes: usize) -> Result<Vec<u8>> {
        let key = self.full_key(path)?;
        // If bytes == 0, then we could return an empty Vec early here.
        // But if someone is requesting zero bytes, then they deserve to have
        // their time and resources wasted by making an unnecessary API call.
        // Do better.
        let _permit = self.acquire_permit().await;
        // Request only the first N bytes using Range header. I've never
        // implemented a Range header, it's wild to me that this works.
        let range = format!("bytes=0-{}", bytes.saturating_sub(1));
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .range(range)
            .send()
            .await
            .map_err(|e| map_get_error(e, path))?;
        let body_bytes = response
            .body
            .collect()
            .await
            .or_raise(|| ErrorKind::Network("failed to read response body".to_string()))?
            .into_bytes();
        Ok(body_bytes.to_vec())
    }

    async fn write(&self, path: &Path, data: &[u8]) -> Result<()> {
        let key = self.full_key(path)?;
        let _permit = self.acquire_permit().await;
        // TODO: Future iteration - implement multipart upload for large files
        //       (>5MB) to improve reliability and allow resumable uploads.
        //       Current implementation uses single PutObject which is fine
        //       for compressed HTML files but may need optimization for
        //       larger content.
        let body = ByteStream::from(data.to_vec());
        self.client.put_object().bucket(&self.bucket).key(&key).body(body).send().await.map_err(|e| match &e {
            SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) => ErrorKind::Network(e.to_string()),
            _ => ErrorKind::BackendError(e.to_string()),
        })?;
        Ok(())
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        let key = self.full_key(path)?;
        // Note: S3 DeleteObject succeeds even if the object doesn't exist.
        // We're only performing the existence check for completeness to match
        // the trait's expected behaviour, not because it's required.
        if !self.exists(path).await? {
            exn::bail!(ErrorKind::NotFound(path.to_path_buf()));
        }
        // Technically because of async, in this little space between the
        // check-then-delete another process (or another task in the runtime)
        // could come along and delete the file. This is a no-op and doesn't
        // fucking matter in the slightest, but I'm learning as I'm going along
        // and I need to remember little things like these for when it does
        // matter (eg, check-then-"some important op" where fn isn't atomic).
        let _permit = self.acquire_permit().await;
        self.client.delete_object().bucket(&self.bucket).key(&key).send().await.map_err(|e| match &e {
            SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) => ErrorKind::Network(e.to_string()),
            _ => ErrorKind::BackendError(e.to_string()),
        })?;
        Ok(())
    }

    // Good lord, why is renaming so complex on S3?
    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let from_key = self.full_key(from)?;
        // Probably redundant, but necessary. See below about `NoSuchKey`.
        if !self.exists(from).await? {
            exn::bail!(ErrorKind::NotFound(from.to_path_buf()));
        }
        let to_key = self.full_key(to)?;
        // S3 doesn't support rename (booo), so we non-atomically copy-then-delete.
        let _permit = self.acquire_permit().await;
        // Copy source format: "bucket/key" (why is this different to the copy
        // target? Why include the bucket name but not the `s3://` prefix?
        // WHY AWS WHY?)
        // TODO: This feels stupid, definitely have to test across multiple
        //       S3-compatible platforms. Tigris, RustFS and Garage maybe?
        let copy_source = format!("{}/{}", self.bucket, from_key);
        self.client.copy_object().bucket(&self.bucket).copy_source(&copy_source).key(&to_key).send().await.map_err(
            |e| match &e {
                // S3 returns a `NoSuchKey` when the source files doesn't exist,
                // but that isn't formally declared in the S3 API spec so
                // therefore it doesn't get modelled in the Rust SDK. The
                // following _should_ work but it not guaranteed to, hence why
                // the above existence check is probably needed even if it's
                // redundant most the time.
                SdkError::ServiceError(s) if matches!(s.err().code(), Some("NoSuchKey")) => {
                    ErrorKind::NotFound(from.to_path_buf())
                },
                SdkError::ServiceError(s) if matches!(s.err(), CopyObjectError::ObjectNotInActiveTierError(_)) => {
                    // WTF am I meant to do with files that do exist but can't be
                    // accessed without incurring fucking ridiculous egress fees?
                    // TODO: Don't crash the application just because you're too lazy to deal with this.
                    unimplemented!("file exists but has fallen deep, deep into the glacier...")
                },
                SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) => ErrorKind::Network(e.to_string()),
                _ => ErrorKind::BackendError(e.to_string()),
            },
        )?;
        // Delete the source object, but log a warning and succeed
        // anyway if this operation fails.
        if let Err(e) = self.client.delete_object().bucket(&self.bucket).key(&from_key).send().await {
            tracing::warn!(source = %from_key, target = %to_key, error = %e, "S3 rename: copy succeed but delete failed, file may be duplicated");
        }
        Ok(())
    }

    async fn stat(&self, path: &Path) -> Result<FileInfo> {
        let key = self.full_key(path)?;
        let _permit = self.acquire_permit().await;
        let response = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| map_head_error(e, path))?;
        let size = response.content_length.unwrap_or(0).max(0) as u64;
        let modified = match response.last_modified {
            Some(ref dt) => Self::parse_datetime(dt)?,
            None => OffsetDateTime::UNIX_EPOCH,
        };
        let compression = Compression::from_path(path);
        Ok(FileInfo::new(path.to_path_buf(), size, modified, compression))
    }
}

fn map_head_error(e: SdkError<HeadObjectError>, path: &Path) -> ErrorKind {
    match &e {
        SdkError::ServiceError(s) if matches!(s.err(), HeadObjectError::NotFound(_)) => {
            ErrorKind::NotFound(path.to_path_buf())
        },
        SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) => ErrorKind::Network(e.to_string()),
        _ => ErrorKind::BackendError(e.to_string()),
    }
}

fn map_get_error(e: SdkError<GetObjectError>, path: &Path) -> ErrorKind {
    match &e {
        SdkError::ServiceError(s) if matches!(s.err(), GetObjectError::NoSuchKey(_)) => {
            ErrorKind::NotFound(path.to_path_buf())
        },
        SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) => ErrorKind::Network(e.to_string()),
        _ => ErrorKind::BackendError(e.to_string()),
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
