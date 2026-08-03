use clap::Parser;
use rawr_cache::{Database, Repository};
use rawr_config::{Config, Loader};
use rawr_web::routes;
use rawr_web::state::AppState;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "rawr-web", version, about = "LAN web interface for your rawr library")]
struct Cli {
    /// Address to bind, eg. 0.0.0.0:8080
    #[arg(short, long, env = "RAWR_WEB_BIND", default_value = "0.0.0.0:8080")]
    bind: SocketAddr,
    /// Path to configuration file
    // No `env` here on purpose: rawr-config's own discovery already
    // honours RAWR_CONFIG (and treats it as required-if-set).
    #[arg(short = 'c', long)]
    config: Option<PathBuf>,
    /// Change the working directory before reading config/cache
    #[arg(short = 'w', long = "working-dir")]
    cwd: Option<PathBuf>,
    /// Serve read-only: uploads are simulated, nothing is written
    #[arg(short = 'd', long, visible_alias = "read-only")]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error}");
            ExitCode::FAILURE
        },
    }
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(ref cwd) = cli.cwd {
        std::env::set_current_dir(cwd)?;
    }
    let (loaded_from, config, warnings) = Config::load::<PathBuf>(cli.config.clone())?;
    tracing::info!(config = %loaded_from.display(), "configuration loaded");
    for warning in &warnings {
        tracing::warn!(path = warning.path, "{}", warning.message);
    }

    let database = Database::connect(config.library.cache.relative()).await?;
    let cache = Repository::new(database.pool().clone(), cli.dry_run);
    let state = AppState::build(&config, cache, cli.dry_run).await?;

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    tracing::info!(
        "listening on http://{} — no authentication; anyone who can reach this port can browse and upload",
        listener.local_addr()?
    );
    if cli.dry_run {
        tracing::info!("dry-run: uploads are simulated, nothing will be written");
    }
    axum::serve(listener, routes::build(state)).with_graceful_shutdown(shutdown_signal()).await?;

    // Runs `PRAGMA optimize` before the pool closes.
    database.close().await;
    Ok(())
}

/// Ctrl-C or SIGTERM (systemd/container stop).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("install Ctrl-C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}

/// Like the CLI's, but keeping timestamps: this is a long-running server
/// and "when" matters in its logs.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("error,rawr=warn,rawr_web=info,html5ever=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).init();
}
