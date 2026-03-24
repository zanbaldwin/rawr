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
#[derive(Clone, Copy, Debug)]
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

pub fn format_error<'a>(
    reason: Reason,
    path: impl Into<Option<&'a str>>,
    message: impl Into<Cow<'a, str>>,
) -> Line<'a> {
    let loudness = reason.loudness();
    let mut line = Line::new([
        Piece::fixed(reason.icon(), reason.style()),
        Piece::space(),
        Piece::fixed(message, reason.style()),
    ])
    .with_volume(loudness);
    if let Some(path) = path.into() {
        line.push(Piece::space());
        line.push(Piece::flex(path, &PALETTE.muted, 32));
    }
    line
}

const AVG_TITLE_LEN: usize = 28;
pub fn format_pair_line<'a, S: HashState + 'a>(
    reason: Reason,
    file: impl Into<Option<&'a FileInfo<S>>>,
    version: &'a Version,
    config: impl Into<Option<&'a FandomConfig>>,
) -> Line<'a> {
    let file = file.into();
    if reason.is_error() {
        let path = file.map(|f| f.path.as_str());
        let msg = reason.icon().trim();
        return format_error(reason, path, msg);
    }
    let config = config.into();
    Line::new(format_pair_pieces(reason, file, version, config))
        .with_volume(reason.loudness())
        .with_fallback(format_pair_fallback(reason, version, config))
}

pub fn format_pair_pieces<'a, S: HashState + 'a>(
    reason: impl Into<Option<Reason>>,
    file: impl Into<Option<&'a FileInfo<S>>>,
    version: &'a Version,
    config: impl Into<Option<&'a FandomConfig>>,
) -> Vec<Piece<'a>> {
    let mut pieces = Vec::with_capacity(13);
    if let Some(reason) = reason.into() {
        pieces.push(Piece::fixed(reason.icon(), reason.style()));
        pieces.push(Piece::space());
    }
    let default_fandom = version.metadata.fandoms.first().map(|f| f.name.as_str());
    let fandom = config
        .into()
        .map(|c| c.preferred_fandom(&version.metadata.fandoms).or(default_fandom))
        .unwrap_or(default_fandom);
    let words_k = version.metadata.words.max(101) as f32 / 1000.0;
    pieces.extend([
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
    ]);
    if let Some(file) = file.into() {
        pieces.push(Piece::space());
        pieces.push(Piece::flex(file.path.as_str(), &PALETTE.muted, 32));
    }
    pieces
}

pub fn format_pair_fallback<'a>(
    reason: impl Into<Option<Reason>>,
    version: &'a Version,
    config: impl Into<Option<&'a FandomConfig>>,
) -> String {
    let default_fandom = version.metadata.fandoms.first().map(|f| f.name.as_str());
    let fandom = config
        .into()
        .map(|c| c.preferred_fandom(&version.metadata.fandoms).or(default_fandom))
        .unwrap_or(default_fandom)
        .map(|f| format!(", in {f}"))
        .unwrap_or_default();
    let words_k = version.metadata.words.max(101) as f32 / 1000.0;
    let fallback = format!(
        "#{} \"{}\"{} ({:.1}k, {})",
        version.metadata.work_id, version.metadata.title, fandom, words_k, version.metadata.chapters
    );
    if let Some(reason) = reason.into() {
        return format!("{} {}", reason.icon(), fallback);
    }
    fallback
}

pub fn format_number(n: u64) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    for &unit in UNITS {
        if size < 1024.0 || unit == "TiB" {
            return if size < 10.0 && unit != "B" {
                format!("{size:.2} {unit}")
            } else if size < 100.0 && unit != "B" {
                format!("{size:.1} {unit}")
            } else {
                format!("{:.0} {unit}", size)
            };
        }
        size /= 1024.0;
    }
    unreachable!()
}
