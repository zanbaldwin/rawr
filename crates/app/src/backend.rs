//! Config-to-storage-backend construction.
//!
//! This is the only place in the workspace that maps a [`TargetConfig`]
//! onto a concrete [`StorageBackend`](rawr_storage::StorageBackend)
//! implementation, including the feature-gated S3 arm and the read-only
//! wrap that keeps dry-run layered rather than per-call.

use crate::error::{ErrorKind, Result};
use exn::{OptionExt, ResultExt};
use rawr_config::Config;
use rawr_config::models::TargetConfig;
use rawr_storage::BackendHandle;
#[cfg(feature = "s3")]
use rawr_storage::backend::S3Backend;
use rawr_storage::backend::{LocalBackend, ReadOnlyBackend};
use std::sync::Arc;

/// The logical role a backend plays, mapped to named targets by
/// `config.library.targets`.
pub enum BackendPurpose {
    Import,
    Export,
    Trash,
}

/// Resolve a backend by its logical purpose.
///
/// `Trash` may be unconfigured, hence the `Option`; `Import` and `Export`
/// are guaranteed present by config validation and auto-fill.
pub async fn backend_by_purpose(
    config: &Config,
    purpose: BackendPurpose,
    read_only: bool,
) -> Result<Option<BackendHandle>> {
    let target_name = match purpose {
        BackendPurpose::Import => Some(&config.library.targets.import),
        BackendPurpose::Export => Some(&config.library.targets.export),
        BackendPurpose::Trash => config.library.targets.trash.as_ref(),
    };
    let Some(target_name) = target_name else {
        return Ok(None);
    };
    backend_by_name(config, target_name, read_only).await.map(Some)
}

/// Resolve a backend by its name in the config's `targets` map.
///
/// When `read_only` is set the handle is wrapped in a
/// [`ReadOnlyBackend`] so every write silently no-ops — dry-run stays
/// layered at construction, not scattered through call sites.
pub async fn backend_by_name(config: &Config, name: impl AsRef<str>, read_only: bool) -> Result<BackendHandle> {
    let target_config =
        config.targets.get(name.as_ref()).ok_or_raise(|| ErrorKind::UnknownTarget(name.as_ref().to_string()))?;
    let mut backend: BackendHandle = match target_config {
        TargetConfig::Local { directory, auto_create } => {
            let path = directory.relative();
            Arc::new(
                LocalBackend::new(name.as_ref(), path.to_string_lossy(), *auto_create)
                    .or_raise(|| ErrorKind::Backend(name.as_ref().to_string()))?,
            )
        },
        #[cfg(not(feature = "s3"))]
        TargetConfig::S3 { .. } => {
            exn::bail!(ErrorKind::UnavailableBackend {
                target: name.as_ref().to_string(),
                feature: "s3",
            });
        },
        #[cfg(feature = "s3")]
        TargetConfig::S3 {
            bucket,
            region,
            endpoint,
            key_id,
            key_secret,
        } => Arc::new(
            S3Backend::new(
                name.as_ref(),
                bucket,
                None::<String>,
                region,
                endpoint.as_deref(),
                key_id.as_ref(),
                key_secret.as_ref(),
            )
            .await
            .or_raise(|| ErrorKind::Backend(name.as_ref().to_string()))?,
        ),
    };
    if read_only {
        backend = Arc::new(ReadOnlyBackend::new(backend));
    }
    Ok(backend)
}
