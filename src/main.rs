#![allow(dead_code)]

mod cli;
mod command;
mod context;
mod error;
mod output;

use crate::cli::{Cli, Commands};
use crate::command::Command;
use crate::context::AppContext;
use crate::error::Result;
use crate::output::{IntoLines, Line, Loudness, PALETTE, Piece, Pipe, PrintingOutput};
use clap::{CommandFactory, Parser};
use console::Term;
use rawr_config::error::ConstraintViolation;
use std::process::ExitCode;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<ExitCode> {
    init_tracing();

    let cli = Cli::parse();
    let output = Arc::new(PrintingOutput::new(cli.color, cli.verbose, cli.quiet));

    // Init runs before context building (no config required).
    if let Some(Commands::Init(command)) = &cli.command {
        return command.execute(output.as_ref()).await;
    }

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
        Commands::Init(_) => unreachable!(),
        Commands::Scan(command) => command.execute(&mut context).await?,
        Commands::Import(command) => command.execute(&mut context).await?,
        Commands::Organize(command) => command.execute(&mut context).await?,
        #[cfg(feature = "_render")]
        Commands::Export(command) => command.execute(&mut context).await?,
        Commands::Stats(command) => command.execute(&mut context).await?,
        Commands::Duplicates(command) => command.execute(&mut context).await?,
    };

    context.shutdown().await;
    Ok(exit)
}

fn print_context(ctx: &AppContext, warnings: &[ConstraintViolation]) {
    let warning_lines = warnings.iter().map(|warning| {
        Line::new([
            Piece::fixed("WARN: ", &PALETTE.warning),
            Piece::fixed(&warning.path, &PALETTE.highlight),
            Piece::space(),
            Piece::plain(&warning.message),
        ])
        .with_volume(Loudness::Shout)
    });
    for line in ctx.to_lines().into_iter().chain(warning_lines) {
        ctx.output.print(Pipe::Err, &line);
    }
    ctx.output.print(Pipe::Err, &Line::empty());
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive("error".parse().unwrap())
        .from_env_lossy()
        .add_directive("rawr=warn".parse().unwrap())
        .add_directive("html5ever=warn".parse().unwrap());
    let with_ansi = PrintingOutput::term_has_colour(&Term::stderr(), &clap::ColorChoice::Auto);
    tracing_subscriber::fmt()
        .without_time()
        .with_ansi(with_ansi)
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .with_target(true)
        .init();
}
