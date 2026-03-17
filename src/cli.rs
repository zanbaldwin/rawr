use crate::command::ImportCommand;
use crate::command::OrganizeCommand;
use crate::command::ScanCommand;
use clap::{ColorChoice, Parser, Subcommand};
use std::path::PathBuf;

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
    Scan(ScanCommand),
    Import(ImportCommand),
    Organize(OrganizeCommand),
}
