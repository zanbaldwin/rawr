use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::command::{OrganizeCommand, ScanCommand};

#[derive(Parser)]
#[command(name = "rawr")]
#[command(version)]
#[command(about = "AO3 fan-fiction library manager")]
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
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Organize(OrganizeCommand),
    Scan(ScanCommand),
}
