use crate::cli::Cli;
use crate::error::Result;
use crate::output::{IntoLines, Line, Loudness, Output, PALETTE, Piece};
use rawr_cache::{Database, Repository};
use rawr_compress::Compression;
use rawr_config::error::ConstraintViolation;
use rawr_config::models::TargetConfig;
use rawr_config::{Config, Loader};
use rawr_library::{Context as LibraryContext, PathGenerator};
use rawr_storage::BackendHandle;
#[cfg(feature = "s3")]
use rawr_storage::backend::S3Backend;
use rawr_storage::backend::{LocalBackend, ReadOnlyBackend};
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) enum BackendPurpose {
    Import,
    Export,
    Trash,
}

pub(crate) struct AppContext {
    pub config: Config,
    loaded_from: PathBuf,
    database: Database,
    pub cache: Repository,
    pub dry_run: bool,
    pub output: Arc<Box<dyn Output>>,
}

impl AppContext {
    pub async fn build(cli: &Cli, output: Box<dyn Output>) -> Result<(Self, Vec<ConstraintViolation>)> {
        if let Some(ref cwd) = cli.cwd {
            std::env::set_current_dir(cwd)?;
        }
        let (loaded_from, config, warnings) = Config::load::<PathBuf>(cli.config.clone())?;
        let database = Database::connect(config.library.cache.relative()).await?;
        let cache = Repository::new(database.pool().clone(), cli.dry_run);
        let ctx = Self {
            config,
            loaded_from,
            database,
            cache,
            dry_run: cli.dry_run,
            output: Arc::new(output),
        };
        Ok((ctx, warnings))
    }

    pub async fn shutdown(&self) {
        self.database.close().await;
    }
}

impl AppContext {
    pub(crate) async fn get_backend_by_purpose(&self, purpose: BackendPurpose) -> Result<Option<BackendHandle>> {
        let target_name = match purpose {
            BackendPurpose::Import => Some(&self.config.library.targets.import),
            BackendPurpose::Export => Some(&self.config.library.targets.export),
            BackendPurpose::Trash => self.config.library.targets.trash.as_ref(),
        };
        let Some(target_name) = target_name else {
            return Ok(None);
        };
        self.get_backend_by_name(target_name).await.map(Some)
    }

    pub(crate) async fn get_backend_by_name(&self, name: impl AsRef<str>) -> Result<BackendHandle> {
        let target_config = self
            .config
            .targets
            .get(name.as_ref())
            .ok_or_else(|| miette::miette!("Config `targets.{}` is not defined.", name.as_ref()))?;
        let mut backend: BackendHandle = match target_config {
            TargetConfig::Local { directory, auto_create } => {
                let path = directory.relative();
                Arc::new(LocalBackend::new(name.as_ref(), path.to_string_lossy(), *auto_create)?)
            },
            #[cfg(not(feature = "s3"))]
            TargetConfig::S3 { .. } => {
                return Err(miette::miette!(
                    help = "Recompile with the `s3` feature enabled to use S3 backends.",
                    "Target `{}` is configured as S3, but S3 support is not available.",
                    name.as_ref(),
                ))?;
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
                .await?,
            ),
        };
        if self.dry_run {
            backend = Arc::new(ReadOnlyBackend::new(backend));
        }
        Ok(backend)
    }

    pub async fn get_library_context(
        &self,
        compression: impl Into<Option<Compression>>,
    ) -> Result<Arc<LibraryContext>> {
        let fandoms = self.config.fandoms.clone();
        let generator = self.config.library.path_templates.import.parse::<PathGenerator>()?;
        let generator = generator.with_fandom_selector(move |fandom_list| {
            let names: Vec<&str> = fandom_list.iter().map(|f| f.name.as_str()).collect();
            fandoms.preferred_fandom(&names).map(String::from)
        });
        Ok(Arc::new(LibraryContext::new(
            generator,
            compression.into(),
            self.get_backend_by_purpose(BackendPurpose::Trash).await?,
            self.dry_run,
        )))
    }
}

impl IntoLines for AppContext {
    fn to_lines(&self) -> Vec<Line<'_>> {
        let cache = self.config.library.cache.relative();
        let import_location = self
            .config
            .targets
            .get(&self.config.library.targets.import)
            // Safety: should already error at config stage if unset.
            .expect("Config requires import target to be defined");
        vec![
            Line::new([Piece::fixed(
                format!(
                    "Config file: {}",
                    self.loaded_from.canonicalize().unwrap_or_else(|_| self.loaded_from.clone()).display()
                ),
                &PALETTE.muted,
            )])
            .with_volume(Loudness::Whisper),
            Line::new([Piece::fixed(
                format!("Main library: {}", import_location),
                &PALETTE.muted,
            )]),
            Line::new([Piece::fixed(
                format!("Cache database: {}", cache.canonicalize().unwrap_or(cache).display()),
                &PALETTE.muted,
            )])
            .with_volume(Loudness::Whisper),
        ]
    }
}
