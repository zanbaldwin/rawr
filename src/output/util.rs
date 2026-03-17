use crate::output::{Line, Loudness, PALETTE, Piece};
use console::Style;
use rawr_config::models::FandomConfig;
use rawr_extract::models::Version;
use rawr_storage::file::{FileInfo, HashState};
use std::borrow::Cow;

/// The reason a line is being printed — determines prefix, color, and loudness.
///
/// Works across scan, organize, and import commands, providing a unified
/// vocabulary for all file-level outcomes.
pub enum Reason {
    /// New content added to the library (scan extraction or first-time import).
    Added,
    /// Newer version of existing content replaced an older one.
    Upgraded,
    /// Already known, nothing changed.
    Unchanged,
    /// Imported a version older than what already exists.
    Outdated,
    /// File moved/renamed to its correct location.
    Moved,
    /// File cleaned up / deleted (duplicate, irreconcilable).
    Removed,

    /// Target path occupied and conflict could not be resolved.
    Conflict,
    /// Not a valid AO3 HTML file (extraction failed).
    InvalidFile,
    /// A dependency (storage, cache, compression, I/O) failed.
    Failed,
}
impl Reason {
    fn icon(&self) -> &'static str {
        match self {
            Self::Added => "+",
            Self::Upgraded => "\u{2191}",
            Self::Unchanged => "=",
            Self::Outdated => "\u{2193}",
            Self::Moved => "\u{00b7}",
            Self::Removed => "\u{00d7}",
            Self::Conflict => "!",
            Self::InvalidFile => "?",
            Self::Failed => "\u{2717}",
        }
    }

    fn style(&self) -> &Style {
        match self {
            Self::Added => &PALETTE.added,
            Self::Upgraded => &PALETTE.success,
            Self::Unchanged => &PALETTE.muted,
            Self::Outdated => &PALETTE.warning,
            Self::Moved => &PALETTE.muted,
            Self::Removed => &PALETTE.danger,
            Self::Conflict => &PALETTE.danger,
            Self::InvalidFile => &PALETTE.warning,
            Self::Failed => &PALETTE.danger,
        }
    }

    pub fn loudness(&self) -> Loudness {
        match self {
            Self::Added | Self::Upgraded | Self::Outdated | Self::Moved => Loudness::Normal,
            Self::Unchanged => Loudness::Whisper,
            Self::Removed | Self::Conflict | Self::InvalidFile | Self::Failed => Loudness::Shout,
        }
    }

    fn is_error(&self) -> bool {
        matches!(self, Self::Conflict | Self::InvalidFile | Self::Failed)
    }
}

pub fn format_error<'a>(reason: Reason, path: &'a str, message: impl Into<Cow<'a, str>>) -> Line<'a> {
    let loudness = reason.loudness();
    Line::new([
        Piece::fixed(reason.icon(), reason.style()),
        Piece::space(),
        Piece::fixed(message, &PALETTE.muted),
        Piece::space(),
        Piece::flex(path, &PALETTE.muted, 32),
    ])
    .with_volume(loudness)
}

const AVG_TITLE_LEN: usize = 28;
pub fn format_pair<'a, S: HashState>(
    reason: Reason,
    file: &'a FileInfo<S>,
    version: &'a Version,
    config: impl Into<Option<&'a FandomConfig>>,
) -> Line<'a> {
    if reason.is_error() {
        let msg = reason.icon().trim();
        return format_error(reason, file.path.as_str(), msg);
    }
    let default_fandom = version.metadata.fandoms.first().map(|f| f.name.as_str());
    let fandom = config
        .into()
        .map(|c| c.preferred_fandom(&version.metadata.fandoms).or(default_fandom))
        .unwrap_or(default_fandom);
    let words_k = version.metadata.words as f32 / 1000.0;
    let line = Line::new([
        Piece::fixed(reason.icon(), reason.style()),
        Piece::space(),
        Piece::fixed(format!("#{}", version.metadata.work_id), &PALETTE.highlight),
        Piece::space(),
        Piece::flex(version.metadata.title.as_str(), &PALETTE.success, AVG_TITLE_LEN),
        Piece::space(),
        Piece::fixed(fandom.unwrap_or_default(), &PALETTE.warning),
        Piece::space(),
        Piece::fixed(format!("{:.1}k", words_k), &PALETTE.highlight),
        Piece::space(),
        Piece::fixed(
            version.metadata.chapters.to_string(),
            if version.metadata.chapters.is_complete() { &PALETTE.success } else { &PALETTE.danger },
        ),
        Piece::space(),
        Piece::flex(file.path.as_str(), &PALETTE.muted, 32),
    ]);
    let fandom = fandom.map(|f| format!(", in {f}")).unwrap_or_default();
    line.with_fallback(format!(
        "{} #{} \"{}\"{} ({:.1}k, {})",
        reason.icon(),
        version.metadata.work_id,
        version.metadata.title,
        fandom,
        words_k,
        version.metadata.chapters
    ))
    .with_volume(reason.loudness())
}
