use std::borrow::{Borrow, Cow};

use console::Style;

use super::Verbosity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pipe {
    Out,
    Err,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Loudness {
    Quiet,
    Normal,
    Loud,
}
impl Loudness {
    pub(crate) fn should_print(&self, verbosity: &Verbosity) -> bool {
        match self {
            Self::Quiet => matches!(verbosity, Verbosity::Verbose),
            Self::Normal => !matches!(verbosity, Verbosity::Quiet),
            Self::Loud => true,
        }
    }
}

pub(crate) enum Flexibility {
    Fixed,
    Truncatable(usize),
}

pub(crate) struct Piece<'a> {
    text: Cow<'a, str>,
    style: Style,
    flex: Flexibility,
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
impl<'a> Piece<'a> {
    pub(crate) fn fixed(text: impl Into<Cow<'a, str>>, style: impl Borrow<Style>) -> Self {
        Self {
            text: text.into(),
            style: style.borrow().clone(),
            flex: Flexibility::Fixed,
        }
    }

    pub(crate) fn flex(text: impl Into<Cow<'a, str>>, style: impl Borrow<Style>, min_width: usize) -> Self {
        Self {
            text: text.into(),
            style: style.borrow().clone(),
            flex: Flexibility::Truncatable(min_width),
        }
    }

    pub(crate) fn space() -> Self {
        Self::plain(" ")
    }

    pub(crate) fn plain(text: impl Into<Cow<'a, str>>) -> Self {
        Self {
            text: text.into(),
            style: Style::new(),
            flex: Flexibility::Fixed,
        }
    }
}

pub(crate) struct Line<'a> {
    pub pipe: Pipe,
    pub loudness: Loudness,
    pieces: Vec<Piece<'a>>,
}

impl<'a> Line<'a> {
    pub(crate) fn new(pipe: Pipe, loudness: Loudness) -> Self {
        Self { pipe, loudness, pieces: Vec::new() }
    }

    pub(crate) fn from_pieces(pipe: Pipe, loudness: Loudness, pieces: impl IntoIterator<Item = Piece<'a>>) -> Self {
        Self {
            pipe,
            loudness,
            pieces: pieces.into_iter().collect(),
        }
    }

    pub(crate) fn push(mut self, piece: Piece<'a>) -> Self {
        self.pieces.push(piece);
        self
    }

    pub(crate) fn render(&self, width: Option<u16>, colors: bool) -> String {
        let mut truncatable_idx = None;
        let mut fixed_width: usize = 0;

        for (i, piece) in self.pieces.iter().enumerate() {
            match piece.flex {
                Flexibility::Fixed => {
                    fixed_width += console::measure_text_width(&piece.text);
                },
                Flexibility::Truncatable(_) => {
                    truncatable_idx = Some(i);
                },
            }
        }

        let apply = |style: &Style, text: &str| -> String {
            if colors { format!("{}", style.apply_to(text)) } else { text.to_string() }
        };

        let mut result = String::new();
        for (i, piece) in self.pieces.iter().enumerate() {
            if Some(i) == truncatable_idx {
                if let Flexibility::Truncatable(min_width) = piece.flex {
                    if let Some(w) = width {
                        let remaining = (w as usize).saturating_sub(fixed_width);
                        if remaining < min_width {
                            continue;
                        }
                        let text_width = console::measure_text_width(&piece.text);
                        if text_width <= remaining {
                            result.push_str(&apply(&piece.style, &piece.text));
                        } else {
                            let truncated = console::truncate_str(&piece.text, remaining, "…");
                            result.push_str(&apply(&piece.style, truncated.as_ref()));
                        }
                    } else {
                        result.push_str(&apply(&piece.style, &piece.text));
                    }
                }
            } else {
                result.push_str(&apply(&piece.style, &piece.text));
            }
        }

        result
    }
}
