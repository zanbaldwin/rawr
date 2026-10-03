use crate::context::{AppContext, BackendPurpose};
use crate::error::Result;
use rawr_cache::Repository;
use rawr_config::models::{FandomConfig, StyleFormat};
use rawr_library::PathGenerator;
use rawr_output::Output;
use rawr_render::StyleConfig;
use rawr_storage::BackendHandle;
use std::sync::Arc;

pub(crate) struct ExportContext<'a> {
    pub(crate) load: BackendHandle,
    pub(crate) save: BackendHandle,
    pub(crate) cache: Arc<Repository>,
    pub(crate) styles: StyleConfig,
    pub(crate) path_generator: PathGenerator,
    pub(crate) output: Arc<dyn Output>,
    pub(crate) fandoms: &'a FandomConfig,
}
impl<'a> ExportContext<'a> {
    /// `format` chooses the stylesheets (`library.styles`).
    pub(crate) async fn try_from_app(ctx: &'a AppContext, format: StyleFormat) -> Result<Self> {
        let path_generator = rawr_app::export_path_generator(&ctx.config)?;

        Ok(Self {
            load: ctx.get_backend_by_purpose(BackendPurpose::Import).await?.ok_or_else(|| {
                miette::miette!(
                    "No import target configured. Define one in your config file under `library.targets.import`."
                )
            })?,
            save: ctx.get_backend_by_purpose(BackendPurpose::Export).await?.ok_or_else(|| {
                miette::miette!(
                    "No export target configured. Define one in your config file under `library.targets.export`."
                )
            })?,
            cache: Arc::new(ctx.cache.clone()),
            styles: rawr_app::styles_for(&ctx.config, format)?,
            path_generator,
            output: Arc::clone(&ctx.output),
            fandoms: &ctx.config.fandoms,
        })
    }
}
