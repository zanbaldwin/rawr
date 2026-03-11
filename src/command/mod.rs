mod organize;
mod scan;

pub(crate) use self::organize::OrganizeCommand;
pub(crate) use self::scan::ScanCommand;
use crate::context::AppContext;
use crate::error::Result;
use rawr_extract::models::{Fandom, Metadata};
use similar::DiffableStr;
use std::borrow::Cow;
use std::process::ExitCode;

pub(crate) trait Command {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode>;
}

/////// TEMPORARY ///////

trait TerminalText<'a> {
    fn display(&self, is_terminal: bool) -> Cow<'a, str>;
    /// Returns the length of the string in **bytes** not characters.
    fn length(&self) -> usize;
}
struct TextPiece<'a> {
    text: Cow<'a, str>,
    code: Option<&'a str>,
}
impl<'a> TextPiece<'a> {
    const SPACE: Self = TextPiece { text: Cow::Borrowed(" "), code: None };

    fn new(text: impl Into<Cow<'a, str>>, code: Option<&'a str>) -> Self {
        Self { text: text.into(), code }
    }
}
impl<'a> TerminalText<'a> for TextPiece<'a> {
    fn display(&self, is_terminal: bool) -> Cow<'a, str> {
        if is_terminal && let Some(code) = self.code.as_ref() {
            Cow::Owned(format!("\x1b[{code}m{}\x1b[{}m", self.text, color::RESET))
        } else {
            self.text.clone()
        }
    }
    fn length(&self) -> usize {
        self.text.as_ref().len()
    }
}
type TerminalLine<'a> = Vec<TextPiece<'a>>;
impl<'a> TerminalText<'a> for TerminalLine<'a> {
    fn display(&self, is_terminal: bool) -> Cow<'a, str> {
        Cow::Owned(self.iter().map(|s| s.display(is_terminal)).collect())
    }
    fn length(&self) -> usize {
        self.iter().map(|s| s.length()).sum()
    }
}

fn format_processed(ctx: &AppContext, metadata: &Metadata, path: Option<impl AsRef<str>>) -> String {
    let work_fandoms: &[Fandom] = &metadata.fandoms;
    let fandom = ctx.config.fandoms.preferred_display_fandom(work_fandoms);
    let mut info = vec![
        TextPiece::new("· ", None),
        TextPiece::new(format!("#{}", metadata.work_id), Some(color::WHITE)),
        TextPiece::SPACE,
        TextPiece::new(metadata.title.as_str(), Some(color::GREEN)),
        TextPiece::SPACE,
        TextPiece::new(fandom.unwrap_or_default(), Some(color::YELLOW)),
        TextPiece::SPACE,
        TextPiece::new(format!("{:.1}k", metadata.words as f32 / 1000.0), Some(color::WHITE)),
        TextPiece::SPACE,
        TextPiece::new(
            metadata.chapters.to_string(),
            Some(if metadata.chapters.is_complete() { color::GREEN } else { color::RED }),
        ),
    ];

    if let Some(path) = path.as_ref().map(|p| p.as_ref()) {
        let remaining = ctx.width.saturating_sub(info.length() + 1);
        if remaining > 8 {
            info.push(TextPiece::SPACE);
            // let path = path.as_ref();
            if path.len() < remaining {
                info.push(TextPiece::new(path, Some(color::GRAY)));
            } else {
                let start = path.ceil_char_boundary(path.len().saturating_sub(remaining));
                info.push(TextPiece::new("…", Some(color::GRAY)));
                info.push(TextPiece::new(path.slice(start..path.len()), Some(color::GRAY)));
            }
        }
    }

    info.display(ctx.use_colour).into()
}

/// ANSI color codes for terminal output.
pub(crate) mod color {
    pub const RESET: &str = "0";
    pub const GREEN: &str = "32";
    pub const YELLOW: &str = "33";
    pub const RED: &str = "31";
    pub const WHITE: &str = "37";
    pub const GRAY: &str = "90";
}
