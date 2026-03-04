use figment::value::magic::RelativePathBuf;
use serde::Deserialize;

use crate::maybe::MaybeFile;

fn default_true() -> bool {
    true
}

/// A named storage backend, discriminated by the `driver` field in config.
///
/// Currently supports local filesystem and S3-compatible object storage.
/// Each variant is validated independently after deserialization — see
/// [`crate::Validator`].
///
/// # Config format
///
/// ```yaml
/// targets:
///   local-example:
///     driver: local
///     directory: /path/to/library
///   s3-example:
///     driver: s3
///     bucket: my-bucket
///     region: us-east-1
///     key_id: file:///run/secrets/aws_key
///     key_secret: file:///run/secrets/aws_secret
/// ```
#[derive(Debug, Deserialize)]
#[serde(tag = "driver")]
pub enum TargetConfig {
    /// Local filesystem storage.
    #[serde(rename = "local")]
    Local {
        /// Root directory for this target. Relative paths are resolved
        /// against the config file's directory.
        directory: RelativePathBuf,
        /// Whether to create `directory` automatically if it doesn't
        /// exist. Defaults to `true`. Set to `false` for removable or
        /// network-mounted storage to get a warning when unmounted.
        #[serde(default = "default_true")]
        auto_create: bool,
    },
    /// S3-compatible object storage (AWS S3, Cloudflare R2, MinIO, etc.).
    #[serde(rename = "s3")]
    S3 {
        /// Bucket name.
        bucket: String,
        /// AWS region or `"auto"` for providers like Cloudflare R2.
        region: String,
        /// Custom endpoint URL for S3-compatible providers. Omit for
        /// standard AWS S3.
        endpoint: Option<String>,
        /// Access key ID. Supports [`MaybeFile`] for secret injection
        /// from files (e.g. Docker secrets).
        key_id: MaybeFile,
        /// Secret access key. Supports [`MaybeFile`] for secret
        /// injection from files.
        key_secret: MaybeFile,
    },
}
