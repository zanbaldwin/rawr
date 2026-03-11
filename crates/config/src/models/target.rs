use crate::maybe::MaybeFile;
use figment::value::magic::RelativePathBuf;
use serde::Deserialize;

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
#[serde(try_from = "TargetValues")]
pub enum TargetConfig {
    /// Local filesystem storage.
    Local {
        /// Root directory for this target. Relative paths are resolved
        /// against the config file's directory.
        directory: RelativePathBuf,
        /// Whether to create `directory` automatically if it doesn't
        /// exist. Defaults to `true`. Set to `false` for removable or
        /// network-mounted storage to get a warning when unmounted.
        auto_create: bool,
    },
    /// S3-compatible object storage (AWS S3, Cloudflare R2, MinIO, etc.).
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

/// Discriminator for [`TargetValues`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TargetDriver {
    Local,
    S3,
}

/// Flat deserialization intermediary for [`TargetConfig`].
///
/// Figment's magic types (like [`RelativePathBuf`]) are incompatible with
/// serde's internally tagged enums because the enum deserialization buffers
/// content into a generic intermediate representation, stripping figment's
/// `Tag` metadata that magic types need to resolve relative paths.
///
/// This flat struct sidesteps the issue: figment deserializes each field
/// directly (no buffering), then [`TryFrom`] converts into the type-safe
/// enum.
#[derive(Debug, Deserialize)]
struct TargetValues {
    driver: TargetDriver,
    // Local fields
    directory: Option<RelativePathBuf>,
    #[serde(default = "default_true")]
    auto_create: bool,
    // S3 fields
    bucket: Option<String>,
    region: Option<String>,
    endpoint: Option<String>,
    key_id: Option<MaybeFile>,
    key_secret: Option<MaybeFile>,
}

impl TryFrom<TargetValues> for TargetConfig {
    type Error = String;

    fn try_from(v: TargetValues) -> Result<Self, Self::Error> {
        match v.driver {
            TargetDriver::Local => {
                let directory = v.directory.ok_or("local target requires `directory`")?;
                Ok(Self::Local { directory, auto_create: v.auto_create })
            },
            TargetDriver::S3 => {
                let bucket = v.bucket.ok_or("s3 target requires `bucket`")?;
                let region = v.region.ok_or("s3 target requires `region`")?;
                let key_id = v.key_id.ok_or("s3 target requires `key_id`")?;
                let key_secret = v.key_secret.ok_or("s3 target requires `key_secret`")?;
                Ok(Self::S3 { bucket, region, endpoint: v.endpoint, key_id, key_secret })
            },
        }
    }
}
