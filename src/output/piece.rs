use crate::output::Render;
use console::Style;
use std::borrow::{Borrow, Cow};

pub enum Flexibility {
    Fixed,
    Truncatable(usize),
}

pub struct Piece<'a> {
    pub(crate) text: Cow<'a, str>,
    pub(crate) style: Style,
    pub(crate) flex: Flexibility,
}
impl<'a> Piece<'a> {
    pub fn fixed(text: impl Into<Cow<'a, str>>, style: impl Borrow<Style>) -> Self {
        Self {
            text: text.into(),
            style: style.borrow().clone(),
            flex: Flexibility::Fixed,
        }
    }

    pub fn flex(text: impl Into<Cow<'a, str>>, style: impl Borrow<Style>, min_width: usize) -> Self {
        Self {
            text: text.into(),
            style: style.borrow().clone(),
            flex: Flexibility::Truncatable(min_width),
        }
    }

    pub fn space() -> Self {
        Self::plain(" ")
    }

    pub fn plain(text: impl Into<Cow<'a, str>>) -> Self {
        Self {
            text: text.into(),
            style: Style::new(),
            flex: Flexibility::Fixed,
        }
    }

    pub(crate) fn width(&self) -> usize {
        console::measure_text_width(&self.text)
    }
}

impl<'a, S> From<(S,)> for Piece<'a>
where
    S: Into<Cow<'a, str>>,
{
    fn from(value: (S,)) -> Self {
        Self::plain(value.0)
    }
}
impl<'a, S, Y> From<(S, Y)> for Piece<'a>
where
    S: Into<Cow<'a, str>>,
    Y: Borrow<Style>,
{
    fn from(value: (S, Y)) -> Self {
        Self::fixed(value.0, value.1)
    }
}
impl<'a, S, Y, W> From<(S, Y, W)> for Piece<'a>
where
    S: Into<Cow<'a, str>>,
    Y: Borrow<Style>,
    W: Into<Option<usize>>,
{
    fn from(value: (S, Y, W)) -> Self {
        let width = value.2.into();
        match width {
            Some(w) => Self::flex(value.0, value.1, w),
            None => Self::fixed(value.0, value.1),
        }
    }
}

impl<'a> Render<'a> for Piece<'a> {
    fn render(&'a self, width: Option<usize>, colour: bool) -> Cow<'a, str> {
        let text = match (width, &self.flex) {
            (Some(max), Flexibility::Truncatable(min)) if self.width() > max => {
                if *min > max {
                    return Cow::Borrowed("");
                }
                console::truncate_str(&self.text, max, "…")
            },
            _ => Cow::Borrowed(self.text.as_ref()),
        };
        if colour { Cow::Owned(self.style.apply_to(text.as_ref()).force_styling(true).to_string()) } else { text }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_creates_fixed_piece_with_default_style() {
        let piece = Piece::plain("hello");
        assert_eq!(piece.text, "hello");
        assert!(matches!(piece.flex, Flexibility::Fixed));
    }

    #[test]
    fn space_creates_plain_space_piece() {
        let piece = Piece::space();
        assert_eq!(piece.text, " ");
        assert!(matches!(piece.flex, Flexibility::Fixed));
    }

    #[test]
    fn fixed_creates_fixed_piece_with_given_style() {
        let style = Style::new().bold();
        let piece = Piece::fixed("hello", &style);
        assert_eq!(piece.text, "hello");
        assert!(matches!(piece.flex, Flexibility::Fixed));
    }

    #[test]
    fn flex_creates_truncatable_piece_with_given_style_and_min_width() {
        let style = Style::new().bold();
        let piece = Piece::flex("hello", &style, 3);
        assert_eq!(piece.text, "hello");
        assert!(matches!(piece.flex, Flexibility::Truncatable(3)));
    }

    #[test]
    fn from_1_tuple_creates_plain_piece() {
        let piece: Piece = ("hello",).into();
        assert_eq!(piece.text, "hello");
        assert!(matches!(piece.flex, Flexibility::Fixed));
    }

    #[test]
    fn from_2_tuple_creates_fixed_piece() {
        let style = Style::new().bold();
        let piece: Piece = ("hello", &style).into();
        assert_eq!(piece.text, "hello");
        assert!(matches!(piece.flex, Flexibility::Fixed));
    }

    #[test]
    fn from_3_tuple_with_direct_width_creates_flex_piece() {
        let style = Style::new().bold();
        let piece: Piece = ("hello", &style, 4).into();
        assert_eq!(piece.text, "hello");
        assert!(matches!(piece.flex, Flexibility::Truncatable(4)));
    }

    #[test]
    fn from_3_tuple_with_some_width_creates_flex_piece() {
        let style = Style::new().bold();
        let piece: Piece = ("hello", &style, Some(4usize)).into();
        assert_eq!(piece.text, "hello");
        assert!(matches!(piece.flex, Flexibility::Truncatable(4)));
    }

    #[test]
    fn from_3_tuple_with_none_width_creates_fixed_piece() {
        let style = Style::new().bold();
        let piece: Piece = ("hello", &style, None::<usize>).into();
        assert_eq!(piece.text, "hello");
        assert!(matches!(piece.flex, Flexibility::Fixed));
    }

    #[test]
    fn render_fixed_no_width_no_colour_returns_text() {
        let piece = Piece::plain("hello");
        assert_eq!(piece.render(None, false), "hello");
    }

    #[test]
    fn render_fixed_with_width_no_colour_never_truncates() {
        let piece = Piece::plain("hello world");
        assert_eq!(piece.render(Some(5), false), "hello world");
    }

    #[test]
    fn render_truncatable_no_width_no_colour_returns_text() {
        let piece = Piece::flex("hello world", Style::new(), 3);
        assert_eq!(piece.render(None, false), "hello world");
    }

    #[test]
    fn render_truncatable_text_fits_within_width() {
        let piece = Piece::flex("hello", Style::new(), 3);
        assert_eq!(piece.render(Some(20), false), "hello");
    }

    #[test]
    fn render_truncatable_text_exceeds_width_truncates() {
        let piece = Piece::flex("hello world", Style::new(), 3);
        let rendered = piece.render(Some(5), false);
        assert!(rendered.len() <= "hello world".len());
        assert!(rendered.contains("…") || rendered.len() <= 5);
        assert_eq!(console::measure_text_width(&rendered), 5);
    }

    #[test]
    fn render_truncatable_min_width_exceeds_max_returns_empty() {
        let piece = Piece::flex("hello world", Style::new(), 10);
        let rendered = piece.render(Some(5), false);
        assert_eq!(rendered, "");
    }

    #[test]
    fn render_no_colour_returns_unstyled_text() {
        let style = Style::new().bold();
        let piece = Piece::fixed("hello", &style);
        assert_eq!(piece.render(None, false), "hello");
    }

    #[test]
    fn render_with_colour_returns_styled_text() {
        let style = Style::new().bold();
        let piece = Piece::fixed("hello", &style);
        let rendered = piece.render(None, true);
        assert_ne!(rendered, "hello");
        assert!(rendered.contains("hello"));
    }
}
