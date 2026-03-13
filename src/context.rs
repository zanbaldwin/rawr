use crate::cli::Cli;
use crate::error::Result;
use crate::output::{IntoLines, Line, Loudness, Output, PALETTE, Piece};
use rawr_cache::{Database, Repository};
use rawr_compress::Compression;
use rawr_config::error::ConstraintViolation;
use rawr_config::models::TargetConfig;
use rawr_config::{Config, Loader};
use rawr_library::Context as LibraryContext;
use rawr_storage::BackendHandle;
use rawr_storage::backend::{LocalBackend, ReadOnlyBackend, S3Backend};
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) struct AppContext {
    pub config: Config,
    database: Database,
    pub cache: Repository,
    pub dry_run: bool,
    pub output: Box<dyn Output>,
}

pub(crate) enum BackendPurpose {
    Import,
    Export,
    Trash,
}

impl AppContext {
    pub async fn build(cli: &Cli, output: Box<dyn Output>) -> Result<(Self, Vec<ConstraintViolation>)> {
        if let Some(ref cwd) = cli.cwd {
            std::env::set_current_dir(cwd)?;
        }
        let (config, warnings) = Config::load::<PathBuf>(cli.config.clone())?;
        let database = Database::connect(config.library.cache.relative()).await?;
        let cache = Repository::new(database.pool().clone(), cli.dry_run);
        let ctx = Self {
            config,
            database,
            cache,
            dry_run: cli.dry_run,
            output,
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
        Ok(Arc::new(LibraryContext::new(
            self.config.library.path_templates.import.parse()?,
            compression.into(),
            self.get_backend_by_purpose(BackendPurpose::Trash).await?,
        )))
    }
}

impl IntoLines for AppContext {
    fn to_lines(&self) -> Vec<Line<'_>> {
        let cache = self.config.library.cache.relative();
        vec![
            Line::new(
                Loudness::Normal,
                [Piece::fixed(
                    format!("Using config file: <not-implemented>"),
                    &PALETTE.muted,
                )],
            ),
            Line::new(
                Loudness::Normal,
                [Piece::fixed(
                    format!("Using cache database: {}", cache.canonicalize().unwrap_or(cache).display()),
                    &PALETTE.muted,
                )],
            ),
        ]
    }
}
