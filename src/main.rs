mod cli;
mod command;
mod context;
mod error;
mod output;

use crate::cli::{Cli, Commands};
use crate::context::AppContext;
use crate::error::Result;
use crate::output::{IntoLines, Line, Loudness, PALETTE, Piece, Pipe, PrintingOutput};
use clap::{CommandFactory, Parser};
use console::Term;
use rawr_config::error::ConstraintViolation;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> Result<ExitCode> {
    init_tracing();

    let cli = Cli::parse();
    let output = Box::new(PrintingOutput::new(cli.color, cli.verbose, cli.quiet));
    let (context, warnings) = AppContext::build(&cli, output).await?;
    print_context(&context, &warnings);

    // Allow the subcommand to be optional, so that we can print the context and
    // config warnings above, before defaulting back to printing the help screen
    // when no subcommand is specified.
    let Some(command) = cli.command else {
        Cli::command().print_help()?;
        context.shutdown().await;
        return Ok(ExitCode::from(2));
    };

    let exit = match command {};

    context.shutdown().await;
    Ok(ExitCode::SUCCESS)
}

fn print_context(ctx: &AppContext, warnings: &[ConstraintViolation]) {
    let warning_lines = warnings.into_iter().map(|warning| {
        Line::new(
            Loudness::Shout,
            [
                Piece::fixed("WARN: ", &PALETTE.warning),
                Piece::fixed(&warning.path, &PALETTE.highlight),
                Piece::space(),
                Piece::plain(&warning.message),
            ],
        )
    });
    for line in ctx.to_lines().into_iter().chain(warning_lines) {
        ctx.output.print(Pipe::Err, &line);
    }
}

const DEFAULT_LOGGING: &str = "rawr=info";
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(DEFAULT_LOGGING.parse().unwrap())
        .from_env_lossy()
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
