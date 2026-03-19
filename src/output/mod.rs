mod render;
mod sink;
mod style;
pub mod util;

pub use self::render::line::{Line, Loudness};
pub use self::render::piece::Piece;
pub use self::render::{IntoLines, Render};
#[cfg(test)]
pub(crate) use self::sink::BufferingOutput;
pub use self::sink::{Output, PrintingOutput};
pub use self::style::PALETTE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pipe {
    Out,
    Err,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}
