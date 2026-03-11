mod cli;
mod command;
mod context;
mod error;

use crate::cli::{Cli, Commands};
use crate::command::Command;
use crate::context::AppContext;
use crate::error::Result;
use clap::{CommandFactory, Parser};
use rawr_config::error::ConstraintViolation;
use std::io::IsTerminal;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> Result<ExitCode> {
    init_tracing();

    let cli = Cli::parse();
    let (mut context, warnings) = AppContext::build(&cli).await?;
    print_context(&context, &warnings);

    // Allow the subcommand to be optional, so that we can print the context and
    // config warnings above, before defaulting back to printing the help screen
    // when no subcommand is specified.
    let Some(command) = cli.command else {
        Cli::command().print_help()?;
        context.shutdown().await;
        return Ok(ExitCode::from(2));
    };

    let exit = match command {
        Commands::Organize(command) => command.execute(&mut context).await?,
        Commands::Scan(command) => command.execute(&mut context).await?,
    };

    context.shutdown().await;
    Ok(exit)
}

// This is a TEMPORARY solution, until we have a more solid TUI process in place.
fn print_context(context: &AppContext, warnings: &[ConstraintViolation]) {
    if context.use_colour {
        eprintln!("\x1b[90m{}\x1b[0m", context);
        for warning in warnings {
            eprintln!("\x1b[38;5;172m· \x1b[37m{}:\x1b[38;5;172m {}\x1b[0m", warning.path, warning.message);
        }
    } else {
        eprintln!("{context}");
        for warning in warnings {
            eprintln!("· {}: {}", warning.path, warning.message);
        }
    }
    eprintln!("");
}

const DEFAULT_LOGGING: &str = "rawr=info";
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(DEFAULT_LOGGING.parse().unwrap())
        .from_env_lossy()
        .add_directive("html5ever=warn".parse().unwrap());
    tracing_subscriber::fmt()
        .without_time()
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .with_target(true)
        .init();
}
