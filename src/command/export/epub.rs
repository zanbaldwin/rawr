use super::File;
use crate::context::AppContext;
use crate::error::Result;
use crate::output::{Line, Loudness, PALETTE, Piece, Pipe};
use rawr_extract::models::Version;
use std::process::ExitCode;

pub(crate) async fn export(ctx: &AppContext, _works: Vec<(Version, Vec<File>)>) -> Result<ExitCode> {
    let line = Line::new([Piece::fixed("EPUB export is not yet implemented", &PALETTE.warning)]);
    ctx.output.print(Pipe::Err, &line.with_volume(Loudness::Shout));
    Ok(ExitCode::FAILURE)
}
