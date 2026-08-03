//! Shared server state, built once at boot via `rawr-app` wiring.

use crate::error::{ErrorKind, Result};
use crate::index::IndexCache;
use exn::{OptionExt, ResultExt};
use rawr_app::BackendPurpose;
use rawr_cache::Repository;
use rawr_config::Config;
use rawr_config::models::FandomConfig;
use rawr_library::{Context as LibraryContext, PathGenerator};
use rawr_render::EpubRenderer;
use rawr_storage::BackendHandle;
use std::ops::Deref;
use std::sync::Arc;

/// Cheap-to-clone handle on everything handlers need.
///
/// `Arc<Inner>` rather than field-wise clone because [`PathGenerator`]
/// isn't `Clone`.
#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}

pub struct Inner {
    pub cache: Repository,
    /// The import target: read source HTML, and the upload sink.
    pub library: BackendHandle,
    /// Import wiring for the upload handler.
    pub library_ctx: Arc<LibraryContext>,
    /// Built once; `Send + Sync`, shared across render tasks.
    pub epub: Arc<EpubRenderer>,
    /// Names EPUB downloads via the user's export template.
    pub export_paths: PathGenerator,
    /// The one target every query is scoped to (`library.targets.import`).
    pub target: String,
    pub fandoms: FandomConfig,
    pub dry_run: bool,
    pub index: IndexCache,
    /// Single-entry render cache keyed `"{cid}-{renderer version}"` —
    /// Safari's range probe re-requests the same EPUB immediately.
    pub epub_cache: tokio::sync::RwLock<Option<(String, axum::body::Bytes)>>,
}

impl AppState {
    pub async fn build(config: &Config, cache: Repository, dry_run: bool) -> Result<Self> {
        let library = rawr_app::backend_by_purpose(config, BackendPurpose::Import, dry_run)
            .await
            .or_raise(|| ErrorKind::Storage)?
            .ok_or_raise(|| ErrorKind::BadRequest("no import target configured"))?;
        let library_ctx = rawr_app::library_context(config, config.library.compression, dry_run)
            .await
            .or_raise(|| ErrorKind::Storage)?;
        // An unconfigured style list still produces sane EPUBs.
        let styles =
            rawr_app::style_config(&config.library.styles, ["builtin:epub.css"]).or_raise(|| ErrorKind::Render)?;
        let export_paths = rawr_app::export_path_generator(config).or_raise(|| ErrorKind::Index)?;
        Ok(Self(Arc::new(Inner {
            cache,
            library,
            library_ctx: Arc::new(library_ctx),
            epub: Arc::new(EpubRenderer::new(styles)),
            export_paths,
            target: config.library.targets.import.clone(),
            fandoms: config.fandoms.clone(),
            dry_run,
            index: IndexCache::new(),
            epub_cache: tokio::sync::RwLock::new(None),
        })))
    }
}
