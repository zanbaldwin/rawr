use crate::output::style::PALETTE;
use crate::output::{line::Line, piece::Piece};
use rawr_config::models::FandomConfig;
use rawr_extract::models::Version;
use rawr_storage::file::{FileInfo, HashState};

const AVG_TITLE_LEN: usize = 28;
pub fn format_pair<'a, S: HashState>(
    file: &'a FileInfo<S>,
    version: &'a Version,
    config: impl Into<Option<&'a FandomConfig>>,
) -> Line<'a> {
    let default_fandom = version.metadata.fandoms.first().map(|f| f.name.as_str());
    let fandom = if let Some(c) = config.into() {
        c.preferred_fandom(&version.metadata.fandoms).or(default_fandom)
    } else {
        default_fandom
    };
    let chapters = version.metadata.chapters.to_string();
    [
        ("· ", &PALETTE.muted).into(),
        (format!("#{}", version.metadata.work_id), &PALETTE.highlight).into(),
        Piece::space(),
        (version.metadata.title.as_str(), &PALETTE.success, AVG_TITLE_LEN).into(),
        Piece::space(),
        (fandom.unwrap_or_default(), &PALETTE.warning).into(),
        Piece::space(),
        (format!("{:.1}k", version.metadata.words as f32 / 1000.0), &PALETTE.highlight).into(),
        Piece::space(),
        (chapters, if version.metadata.chapters.is_complete() { &PALETTE.success } else { &PALETTE.danger }).into(),
        Piece::space(),
        (file.path.as_str(), &PALETTE.muted, 32).into(),
    ]
    .into()
}
