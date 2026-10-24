use crate::document::{Document, DocumentMeta, Origin, Rating, Section};
use crate::error::{ErrorKind, Result};
use crate::source::Source;
use exn::{OptionExt, ResultExt};
use rslug::slugify;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;

const IDENTIFIER_PREFIX: &str = "urn:rawr-book:";

/// [`Source`] for structured books in YAML: chapters of paragraphs of
/// sentences, each sentence in one or more languages.
///
/// The body XHTML carries narration markers: `data-rawr-voice` on a paragraph
/// whose speaker has a voice, `data-rawr-sentence` around each sentence, and
/// `data-rawr-speak="never"` on speaker labels and translations. With a
/// translation language, each sentence and speaker label gets its
/// translation in `<ruby>`/`<rt>`. Both editions have the same spoken text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookYaml {
    /// The language that is shown and spoken.
    pub language: String,
    /// The language shown as ruby annotations, if any.
    pub translation: Option<String>,
}

/// A voice listed in the book. No name means the narrator.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Voice {
    pub name: Option<String>,
    pub voice: String,
}

/// Book-wide settings that are not part of the [`Document`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSettings {
    /// The language that is shown and spoken, when the book says.
    pub language: Option<String>,
    /// What to highlight while the audio plays (for example `word` or `sentence`), when the book says.
    pub highlight: Option<String>,
    pub voices: Vec<Voice>,
}

/// A text that is the same in every language, or one text per language.
#[derive(Deserialize)]
#[serde(untagged)]
enum Text {
    Plain(String),
    #[serde(deserialize_with = "languages")]
    Languages(BTreeMap<String, String>),
}

#[derive(Deserialize)]
struct Book {
    title: Text,
    identifier: Option<String>,
    language: Option<String>,
    highlight: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    summary: Option<Text>,
    rating: Option<String>,
    #[serde(default)]
    voices: Vec<Voice>,
    chapters: Vec<Chapter>,
}

#[derive(Deserialize)]
struct Chapter {
    title: Option<Text>,
    #[serde(default)]
    paragraphs: Vec<Paragraph>,
}

#[derive(Deserialize)]
struct Paragraph {
    character: Option<Text>,
    #[serde(default)]
    direction: bool,
    sentences: Vec<Text>,
}

impl BookYaml {
    pub fn new(language: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            translation: None,
        }
    }

    pub fn with_translation(mut self, translation: impl Into<String>) -> Self {
        self.translation = Some(translation.into());
        self
    }

    /// The voices the book lists.
    ///
    /// # Errors
    ///
    /// [`Extract`](ErrorKind::Extract) if the input is not a valid book.
    pub fn voices(&self, input: &[u8]) -> Result<Vec<Voice>> {
        Ok(Self::settings(input)?.voices)
    }

    /// The book-wide settings: language, highlight and voices.
    ///
    /// # Errors
    ///
    /// [`Extract`](ErrorKind::Extract) if the input is not a valid book.
    pub fn settings(input: &[u8]) -> Result<BookSettings> {
        let book = parse(input)?;
        Ok(BookSettings {
            language: book.language,
            highlight: book.highlight,
            voices: book.voices,
        })
    }

    fn paragraph(&self, paragraph: &Paragraph, voices: &[Voice]) -> Result<String> {
        let voice = paragraph.character.as_ref().filter(|_| !paragraph.direction).and_then(|character| {
            voices
                .iter()
                .filter_map(|voice| voice.name.as_deref())
                .find(|name| character.values().any(|value| value == *name))
        });
        let mut out = String::from(if paragraph.direction { "<p class=\"direction\"" } else { "<p" });
        if let Some(voice) = voice {
            out.push_str(&format!(" data-rawr-voice=\"{}\"", escape(voice)));
        }
        out.push('>');
        let mut pieces = Vec::new();
        if let Some(character) = &paragraph.character {
            pieces.push(format!(
                "<span class=\"speaker\" data-rawr-speak=\"never\">{}</span>",
                self.annotated(character, false)?
            ));
        }
        for sentence in &paragraph.sentences {
            pieces.push(self.annotated(sentence, true)?);
        }
        out.push_str(&pieces.join(" "));
        out.push_str("</p>");
        Ok(out)
    }

    /// One text in the book's language, with its translation as ruby when
    /// there is one. A sentence is wrapped in a `data-rawr-sentence` span.
    fn annotated(&self, text: &Text, sentence: bool) -> Result<String> {
        let shown = self.text(text)?;
        let base =
            if sentence { format!("<span data-rawr-sentence=\"\">{}</span>", escape(shown)) } else { escape(shown) };
        Ok(
            match self.translation.as_deref().and_then(|language| match text {
                Text::Plain(_) => None,
                Text::Languages(texts) => texts.get(language),
            }) {
                Some(translation) => {
                    format!("<ruby>{base}<rt data-rawr-speak=\"never\">{}</rt></ruby>", escape(translation))
                },
                None => base,
            },
        )
    }

    /// The text in the book's language.
    ///
    /// # Errors
    ///
    /// [`Extract`](ErrorKind::Extract) if the text is not given in the book's language.
    fn text<'a>(&self, text: &'a Text) -> Result<&'a str> {
        text.get(&self.language).ok_or_raise(|| ErrorKind::Extract)
    }
}

impl Source for BookYaml {
    fn accepts(&self, input: &[u8]) -> bool {
        std::str::from_utf8(input).is_ok_and(|text| text.lines().any(|line| line.starts_with("chapters:")))
    }

    fn to_document(&self, input: &[u8]) -> Result<Document> {
        if !self.accepts(input) {
            exn::bail!(ErrorKind::Unsupported);
        }
        let book = parse(input)?;
        if book.chapters.is_empty() {
            exn::bail!(ErrorKind::NoSections);
        }
        let mut sections = Vec::with_capacity(book.chapters.len());
        for (index, chapter) in book.chapters.iter().enumerate() {
            let paragraphs = chapter
                .paragraphs
                .iter()
                .map(|paragraph| self.paragraph(paragraph, &book.voices))
                .collect::<Result<Vec<_>>>()?;
            sections.push(Section {
                number: index as u32 + 1,
                title: chapter.title.as_ref().map(|title| self.text(title).map(String::from)).transpose()?,
                summary: None,
                body_xhtml: paragraphs.join("\n"),
                notes: None,
                end_notes: None,
            });
        }
        let title = self.text(&book.title)?.to_string();
        Ok(Document {
            meta: DocumentMeta {
                identifier: book.identifier.unwrap_or_else(|| format!("{IDENTIFIER_PREFIX}{}", slugify!(&title))),
                title,
                authors: book.authors,
                language: Some(self.language.clone()),
                summary: book.summary.as_ref().map(|summary| self.text(summary).map(String::from)).transpose()?,
                rating: match book.rating.as_deref().map(str::to_ascii_lowercase).as_deref() {
                    Some("general") => Rating::General,
                    Some("teen") => Rating::Teen,
                    Some("mature") => Rating::Mature,
                    Some("explicit") => Rating::Explicit,
                    _ => Rating::Unrated,
                },
            },
            sections,
            origin: Origin::Generic,
        })
    }
}

impl Text {
    /// The text in a language. A plain text is the same in every language.
    fn get(&self, language: &str) -> Option<&str> {
        match self {
            Self::Plain(plain) => Some(plain),
            Self::Languages(texts) => texts.get(language).map(String::as_str),
        }
    }

    fn values(&self) -> impl Iterator<Item = &str> {
        let (plain, texts) = match self {
            Self::Plain(plain) => (Some(plain.as_str()), None),
            Self::Languages(texts) => (None, Some(texts.values().map(String::as_str))),
        };
        plain.into_iter().chain(texts.into_iter().flatten())
    }
}

/// A map of language to text, with at least one entry.
fn languages<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<BTreeMap<String, String>, D::Error> {
    let texts = BTreeMap::<String, String>::deserialize(deserializer)?;
    if texts.is_empty() {
        return Err(D::Error::invalid_length(0, &"at least one language"));
    }
    Ok(texts)
}

fn parse(input: &[u8]) -> Result<Book> {
    serde_saphyr::from_slice(input).or_raise(|| ErrorKind::Extract)
}

/// The same escaping as rawr-xhtml: `&`, `<`, `>`, `"`, and the no-break space as `&#160;`.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\u{a0}' => out.push_str("&#160;"),
            other => out.push(other),
        }
    }
    out
}
