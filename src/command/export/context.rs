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
    /// `format` chooses the stylesheets (`library.styles`). For audiobooks, the
    /// save target and path template are the audiobook ones.
    pub(crate) async fn try_from_app(ctx: &'a AppContext, format: StyleFormat) -> Result<Self> {
        let (path_generator, purpose, key) = match format {
            StyleFormat::Audiobook => (rawr_app::audio_path_generator(&ctx.config)?, BackendPurpose::Audio, "audio"),
            StyleFormat::Pdf | StyleFormat::Epub => {
                (rawr_app::export_path_generator(&ctx.config)?, BackendPurpose::Export, "export")
            },
        };

        Ok(Self {
            load: ctx.get_backend_by_purpose(BackendPurpose::Import).await?.ok_or_else(|| {
                miette::miette!(
                    "No import target configured. Define one in your config file under `library.targets.import`."
                )
            })?,
            save: ctx.get_backend_by_purpose(purpose).await?.ok_or_else(|| {
                miette::miette!(
                    "No {key} target configured. Define one in your config file under `library.targets.{key}`."
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
