mod organize;
mod scan;

pub(crate) use self::organize::OrganizeCommand;
pub(crate) use self::scan::ScanCommand;
use crate::context::AppContext;
use crate::error::Result;
use crate::output::{Palette, Piece};
use rawr_extract::models::Metadata;
use std::process::ExitCode;

pub(crate) trait Command {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode>;
}

fn stylize_metadata<'a>(metadata: &'a Metadata, path: Option<impl AsRef<str> + 'a>) -> Vec<Piece<'a>> {
    let palette = Palette::default();
    let fandom = metadata.fandoms.first().map(|f| f.name.as_str());
    let chapters = metadata.chapters.to_string();
    let mut pieces: Vec<Piece<'_>> = vec![
        ("· ", &palette.muted).into(),
        (format!("#{}", metadata.work_id), &palette.highlight).into(),
        Piece::space(),
        (metadata.title.as_str(), &palette.success).into(),
        Piece::space(),
        (fandom.unwrap_or_default(), &palette.warning).into(),
        (format!(" {:.1}k ", metadata.words as f32 / 1000.0), &palette.highlight).into(),
        (chapters, if metadata.chapters.is_complete() { &palette.success } else { &palette.danger }).into(),
    ];
    if let Some(path) = path {
        pieces.push(Piece::space());
        pieces.push((path.as_ref().to_owned(), &palette.muted, 8usize).into());
    }
    pieces
}
