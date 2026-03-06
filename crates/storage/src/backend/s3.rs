//! S3-compatible storage backend.
//!
//! This module provides a storage backend implementation for S3-compatible
//! services including AWS S3, Backblaze B2, Tigris (Fly.io), and others,
//! using [OpenDAL](https://docs.rs/opendal/) with the `S3` service.
//!
//! # Credentials
//!
//! Credentials are provided explicitly via the configuration file. Each
//! target specifies its own `key_id` and `key_secret`.

use super::opendal_util::map_opendal_error;
use crate::backend::OperatorAware;
use crate::error::{ErrorKind, Result};
use crate::{StorageBackend, ValidatedPath};
use async_trait::async_trait;
use futures::{AsyncWriteExt, io::copy as async_copy};
use opendal::Operator;
use opendal::layers::{ConcurrentLimitLayer, RetryLayer};
use opendal::services::S3;
use std::path::Path;

/// S3-compatible storage backend.
///
/// Stores files in an S3(-compatible) bucket, optionally under a key prefix.
/// All paths are relative to the configured prefix (if any).
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
    operator: Operator,
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
        let mut builder = S3::default()
            .bucket(&bucket.into())
            .region(&region.into())
            .access_key_id(&key_id.into())
            .secret_access_key(&key_secret.into());

        if let Some(ep) = endpoint {
            builder = builder.endpoint(&ep.into());
        }
        if let Some(pfx) = prefix {
            let root = ValidatedPath::new(&pfx)?;
            builder = builder.root(root.as_str());
        }

        let operator = Operator::new(builder)
            .map_err(|e| ErrorKind::BackendError(e.to_string()))?
            .layer(RetryLayer::default().with_max_times(4))
            .layer(ConcurrentLimitLayer::new(100))
            .finish();

        Ok(Self { name: name.into(), operator })
    }
}

impl OperatorAware for S3Backend {
    fn operator(&self) -> &Operator {
        &self.operator
    }
}
#[async_trait]
impl StorageBackend for S3Backend {
    fn name(&self) -> &str {
        &self.name
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let validated_from = ValidatedPath::new(from)?;
        let validated_to = ValidatedPath::new(to)?;
        // S3 doesn't support rename natively. OpenDAL may implement it via
        // copy+delete, or we may need to do it ourselves.
        match self.operator.rename(validated_from.as_str(), validated_to.as_str()).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == opendal::ErrorKind::Unsupported => {
                // Fallback: copy then delete (same approach as prior aws-sdk-s3 impl)
                if !self.exists(from).await? {
                    exn::bail!(ErrorKind::NotFound(from.to_path_buf()));
                }
                let mut reader = self.reader(from).await?;
                let mut writer = self.writer(to).await?;
                async_copy(&mut reader, &mut writer).await.map_err(ErrorKind::Io)?;
                writer.close().await.map_err(ErrorKind::Io)?;
                if let Err(e) = self.operator.delete(validated_from.as_str()).await {
                    tracing::warn!(
                        source = %from.display(), target = %to.display(), error = %e,
                        "S3 rename: copy succeeded but delete failed, file may be duplicated"
                    );
                }
                Ok(())
            },
            Err(e) => Err(map_opendal_error(e, from).into()),
        }
    }
}
