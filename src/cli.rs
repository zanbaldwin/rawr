use crate::command::ImportCommand;
use crate::command::OrganizeCommand;
use crate::command::ScanCommand;
use crate::command::StatsCommand;
use clap::{ColorChoice, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rawr")]
#[command(version)]
#[command(about = "AO3 fan-fiction library manager")]
#[command(long_about = r#"AO3 fan-fiction library manager.

Manage a local library of downloaded AO3 HTML files. Typical workflow:
  1. rawr scan                    Run an initial scan on your library
  2. rawr import ~/Downloads -r   Import downloaded files
  3. rawr organize                Move files around in the library to the right locations"#)]
#[command(arg_required_else_help = false)]
pub(crate) struct Cli {
    /// Path to configuration file
    #[arg(short = 'c', long = "config", global = true)]
    pub config: Option<PathBuf>,
    /// Change the working directory before reading config/cache/download files
    #[arg(short = 'w', long = "working-dir", global = true)]
    pub cwd: Option<PathBuf>,
    /// Preview changes without executing
    #[arg(short = 'd', long = "dry-run", global = true, visible_alias = "read-only")]
    pub dry_run: bool,
    /// Enable verbose output
    #[arg(short = 'v', long, global = true)]
    pub verbose: bool,
    /// Suppress all output except errors
    #[arg(short = 'q', long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,
    /// Control color output [auto, always, never]
    #[arg(long, global = true, default_value = "auto")]
    pub color: ColorChoice,
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Scan your import library for HTML files and extract metadata into the cache
    Scan(ScanCommand),
    /// Import HTML files from the local filesystem (eg, your downloads folder) into your library
    Import(ImportCommand),
    /// Reorganize files in the import target to match the configured location/compression
    Organize(OrganizeCommand),
    /// Show library statistics
    Stats(StatsCommand),
}
