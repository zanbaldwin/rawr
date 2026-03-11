mod cli;
mod command;
mod context;
mod error;
mod output;

use crate::cli::{Cli, Commands};
use crate::command::Command;
use crate::context::AppContext;
use crate::error::Result;
use crate::output::{Line, Loudness, Palette, Pipe, TerminalOutput};
use clap::{CommandFactory, Parser};
use rawr_config::error::ConstraintViolation;
use std::io::IsTerminal;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> Result<ExitCode> {
    init_tracing();

    let cli = Cli::parse();
    let output = Box::new(TerminalOutput::new(cli.color, cli.verbose, cli.quiet));
    let (mut context, warnings) = AppContext::build(&cli, output).await?;
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

fn print_context(ctx: &AppContext, warnings: &[ConstraintViolation]) {
    let palette = Palette::default();
    // TODO: Should Loudness be part of the line definition, or part of the print call?
    ctx.output.print(Pipe::Err, &Line::new(Loudness::Quiet).push((format!("{ctx}"), &palette.muted).into()));
    for warning in warnings {
        ctx.output.print(
            Pipe::Err,
            &Line::from_pieces(
                Loudness::Loud,
                [
                    ("warning: ", &palette.warning).into(),
                    (format!("{}: {}", warning.path, warning.message),).into(),
                ],
            ),
        );
    }
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
