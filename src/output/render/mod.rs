pub(super) mod line;
pub(super) mod piece;

use self::line::Line;
use std::borrow::Cow;

pub trait Render<'a> {
    fn render(&'a self, width: Option<usize>, colour: bool) -> Cow<'a, str>;
}

pub trait IntoLines {
    fn to_lines(&self) -> Vec<Line<'_>>;
}
