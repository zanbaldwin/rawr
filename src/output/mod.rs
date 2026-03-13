mod line;
mod piece;

use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

pub(crate) trait Render<'a> {
    fn render(&'a self, width: Option<usize>, colour: bool) -> Cow<'a, str>;
}
