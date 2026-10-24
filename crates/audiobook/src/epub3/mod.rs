//! EPUB 3 with Media Overlays.
//!
//! One XHTML page, one SMIL overlay and one audio file per narrated section.
//! The page text is the section's XHTML exactly as rawr-xhtml wrote it,
//! with a `<span id>` around every sentence (see [`chapter_body`]), and
//! around every word with [`SyncLevel::Word`].

mod body;
mod xml;

use crate::audio::{EncodedAudio, SAMPLE_RATE};
use crate::error::{ErrorKind, Result};
use crate::plan::SectionPlan;
use crate::script::Script;
use crate::timeline::{SyncUnit, UnitRef};
use exn::ResultExt;
use rawr_render::StyleConfig;
use rawr_render::pages::chapter_heading;
use rawr_source::Document;
#[cfg(feature = "ao3")]
use rawr_source::Origin;
use rawr_tts::Fingerprint;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Result as FmtResult, Write as _};
use std::io::{Seek, Write};
use std::str::FromStr;
use time::{OffsetDateTime, UtcOffset};
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

/// The class a reading system puts on the sentence being read. The book has
/// no rule for it: reading apps (and user stylesheets) choose the highlight.
pub const ACTIVE_CLASS: &str = "-epub-media-overlay-active";

/// What the reading system highlights while the audio plays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SyncLevel {
    /// One clip per sentence.
    #[default]
    Sentence,
    /// One clip per word, grouped by sentence. Sentences are `<span class="s">`
    /// and words `<span class="w">`, so a stylesheet can mark both, for example
    /// `.s:has(.-epub-media-overlay-active)` for the sentence being read.
    Word,
}

impl Display for SyncLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(match self {
            Self::Sentence => "sentence",
            Self::Word => "word",
        })
    }
}

impl FromStr for SyncLevel {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "sentence" | "sentences" => Ok(Self::Sentence),
            "word" | "words" => Ok(Self::Word),
            other => Err(format!("unknown sync level \"{other}\" (expected \"sentence\" or \"word\")")),
        }
    }
}

/// One narrated section: its plan, its sync map, and its encoded audio.
///
/// It holds no PCM: drop the [`ChapterAudio`](crate::timeline::ChapterAudio)
/// once its audio is encoded.
pub struct Chapter<'a> {
    pub section: &'a SectionPlan,
    /// [`ChapterAudio::units`](crate::timeline::ChapterAudio::units).
    pub units: Vec<SyncUnit>,
    /// [`ChapterAudio::words`](crate::timeline::ChapterAudio::words). Used with [`SyncLevel::Word`].
    pub words: Vec<SyncUnit>,
    pub encoded: EncodedAudio,
}

/// Everything in one audiobook.
pub struct Package<'a> {
    pub document: &'a Document,
    /// Narrated sections. Sections of the document without one (nothing to
    /// speak) are still in the book, as text only.
    pub chapters: Vec<Chapter<'a>>,
    pub styles: &'a StyleConfig,
    /// The narrator credit, for example from [`narrator_credit`].
    pub narrator: Option<String>,
    /// `dcterms:modified`.
    pub modified: OffsetDateTime,
    /// What is highlighted.
    pub sync: SyncLevel,
    /// The `dc:identifier`, when it differs from the document's (for example
    /// a second edition of the same narration).
    pub identifier: Option<String>,
}

/// "{voice name} ({provider} {model})", or "{provider} {model}" when the name is not known.
pub fn narrator_credit(provider: &str, fingerprint: &Fingerprint, voice_name: Option<&str>) -> String {
    let provider = match provider {
        "elevenlabs" => "ElevenLabs",
        "fake" => "Fake",
        other => other,
    };
    let model = fingerprint.as_str().split(';').find_map(|entry| entry.strip_prefix("model="));
    let engine = match model {
        Some(model) => format!("{provider} {model}"),
        None => provider.to_string(),
    };
    match voice_name {
        Some(name) => format!("{name} ({engine})"),
        None => engine,
    }
}

/// A section's body XHTML with a `<span id="{chapter}-s0001">` around every
/// sentence of its script. The text and block structure are unchanged;
/// inline elements are split only where a sentence crosses them.
pub fn chapter_body(script: &Script, body_xhtml: &str, chapter: &str) -> String {
    chapter_body_at(script, body_xhtml, chapter, SyncLevel::Sentence)
}

/// [`chapter_body`] at a [`SyncLevel`]. With [`SyncLevel::Word`], each
/// sentence span has `class="s"` and holds a `<span id="{sentence}-w001"
/// class="w">` around every word.
pub fn chapter_body_at(script: &Script, body_xhtml: &str, chapter: &str, sync: SyncLevel) -> String {
    let ids = body::sentence_ids(script, chapter);
    match sync {
        SyncLevel::Sentence => body::with_spans(body_xhtml, script, &ids, None),
        SyncLevel::Word => body::with_spans(body_xhtml, script, &ids, Some(&body::word_ids(script, &ids))),
    }
}

struct Page {
    id: String,
    label: String,
    xhtml: String,
    overlay: Option<Overlay>,
}

struct Overlay {
    smil: String,
    audio: Vec<u8>,
    audio_href: String,
    media_type: &'static str,
    milliseconds: u64,
}

impl Package<'_> {
    /// Write the book as a zip (EPUB container) and return the writer.
    ///
    /// # Errors
    ///
    /// [`Package`](ErrorKind::Package) if writing fails.
    pub fn write<W: Write + Seek>(&self, writer: W) -> Result<W> {
        let meta = &self.document.meta;
        let language = meta.language.clone().unwrap_or_else(|| "und".to_string());
        let multi_chapter = self.document.sections.len() > 1;
        let narrated: BTreeMap<u32, &Chapter<'_>> =
            self.chapters.iter().map(|chapter| (chapter.section.script.section, chapter)).collect();
        let mut pages = Vec::new();
        for section in &self.document.sections {
            let id = format!("c{:03}", section.number);
            let heading = chapter_heading(section.number, section.title.as_deref(), multi_chapter);
            let label = heading.clone().unwrap_or_else(|| meta.title.clone());
            let (body, overlay) = match narrated.get(&section.number) {
                Some(chapter) => {
                    let script = &chapter.section.script;
                    let mut body = String::new();
                    if let Some(heading) = &script.heading {
                        writeln!(
                            body,
                            "<h2 class=\"chapter-title\"><span id=\"{id}-h\">{}</span></h2>",
                            xml::text(&heading.text)
                        )
                        .expect("writing to a String");
                    } else if let Some(heading) = &heading {
                        writeln!(body, "<h2 class=\"chapter-title\">{}</h2>", xml::text(heading))
                            .expect("writing to a String");
                    }
                    body.push_str(&chapter_body_at(script, &section.body_xhtml, &id, self.sync));
                    (body, Some(overlay(&id, chapter, self.sync)))
                },
                None => {
                    let mut body = String::new();
                    if let Some(heading) = &heading {
                        writeln!(body, "<h2 class=\"chapter-title\">{}</h2>", xml::text(heading))
                            .expect("writing to a String");
                    }
                    body.push_str(&section.body_xhtml);
                    (body, None)
                },
            };
            pages.push(Page {
                xhtml: page(&language, &label, &body),
                id,
                label,
                overlay,
            });
        }
        let mut css = Vec::new();
        self.styles
            .write_all_to::<Option<&[u8]>, Option<&[u8]>, &[u8], &[u8]>(&mut css, (None, Some(b"\n".as_slice())))
            .or_raise(|| ErrorKind::Package)?;
        let title_page = page(&language, &meta.title, &self.title_page());
        let opf = self.opf(&language, &pages)?;
        let nav = nav(&language, &pages);

        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let mut zip = ZipWriter::new(writer);
        let file = |zip: &mut ZipWriter<W>, name: &str, options: SimpleFileOptions, bytes: &[u8]| -> Result<()> {
            zip.start_file(name, options).or_raise(|| ErrorKind::Package)?;
            zip.write_all(bytes).or_raise(|| ErrorKind::Package)
        };
        file(&mut zip, "mimetype", stored, b"application/epub+zip")?;
        file(&mut zip, "META-INF/container.xml", deflated, CONTAINER.as_bytes())?;
        file(&mut zip, "OEBPS/content.opf", deflated, opf.as_bytes())?;
        file(&mut zip, "OEBPS/nav.xhtml", deflated, nav.as_bytes())?;
        file(&mut zip, "OEBPS/stylesheet.css", deflated, &css)?;
        file(&mut zip, "OEBPS/title.xhtml", deflated, title_page.as_bytes())?;
        for page in &pages {
            file(&mut zip, &format!("OEBPS/{}.xhtml", page.id), deflated, page.xhtml.as_bytes())?;
            if let Some(overlay) = &page.overlay {
                file(&mut zip, &format!("OEBPS/{}.smil", page.id), deflated, overlay.smil.as_bytes())?;
                file(&mut zip, &format!("OEBPS/{}", overlay.audio_href), stored, &overlay.audio)?;
            }
        }
        zip.finish().or_raise(|| ErrorKind::Package)
    }

    fn title_page(&self) -> String {
        let meta = &self.document.meta;
        let mut body = match &self.document.origin {
            #[cfg(feature = "ao3")]
            Origin::Ao3(version) => rawr_render::pages::title_page_body(&version.metadata),
            _ => {
                let byline = if meta.authors.is_empty() { "Anonymous".to_string() } else { meta.authors.join(", ") };
                let mut body = format!(
                    "<div class=\"title-page\">\n<h1 class=\"work-title\">{}</h1>\n<p class=\"byline\">by {}</p>\n",
                    xml::text(&meta.title),
                    xml::text(&byline)
                );
                if let Some(summary) = &meta.summary {
                    body.push_str("<div class=\"summary\">\n");
                    for paragraph in summary.split("\n\n").map(str::trim).filter(|paragraph| !paragraph.is_empty()) {
                        writeln!(body, "<p>{}</p>", xml::text(paragraph)).expect("writing to a String");
                    }
                    body.push_str("</div>\n");
                }
                body.push_str("</div>");
                body
            },
        };
        if let Some(narrator) = &self.narrator {
            write!(body, "\n<p class=\"narrator\">Narrated by {}</p>", xml::text(narrator))
                .expect("writing to a String");
        }
        body
    }

    fn opf(&self, language: &str, pages: &[Page]) -> Result<String> {
        let meta = &self.document.meta;
        let utc = self.modified.to_offset(UtcOffset::UTC);
        let modified = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            utc.year(),
            u8::from(utc.month()),
            utc.day(),
            utc.hour(),
            utc.minute(),
            utc.second()
        );
        let mut metadata = String::new();
        let mut line = |text: String| {
            metadata.push_str("    ");
            metadata.push_str(&text);
            metadata.push('\n');
        };
        let identifier = self.identifier.as_ref().unwrap_or(&meta.identifier);
        line(format!("<dc:identifier id=\"uid\">{}</dc:identifier>", xml::text(identifier)));
        line(format!("<dc:title>{}</dc:title>", xml::text(&meta.title)));
        line(format!("<dc:language>{}</dc:language>", xml::text(language)));
        let authors = if meta.authors.is_empty() { vec!["Anonymous".to_string()] } else { meta.authors.clone() };
        for (index, author) in authors.iter().enumerate() {
            line(format!("<dc:creator id=\"creator{}\">{}</dc:creator>", index + 1, xml::text(author)));
        }
        if let Some(summary) = &meta.summary {
            line(format!("<dc:description>{}</dc:description>", xml::text(summary)));
        }
        if meta.identifier.starts_with("https://") || meta.identifier.starts_with("http://") {
            line(format!("<dc:source>{}</dc:source>", xml::text(&meta.identifier)));
        }
        line(format!("<meta property=\"dcterms:modified\">{modified}</meta>"));
        if let Some(narrator) = &self.narrator {
            line(format!("<dc:contributor id=\"narrator\">{}</dc:contributor>", xml::text(narrator)));
            line("<meta refines=\"#narrator\" property=\"role\" scheme=\"marc:relators\">nrt</meta>".to_string());
            line(format!("<meta property=\"media:narrator\">{}</meta>", xml::text(narrator)));
        }
        let mut total = 0;
        for page in pages {
            if let Some(overlay) = &page.overlay {
                total += overlay.milliseconds;
                line(format!(
                    "<meta property=\"media:duration\" refines=\"#{}-mo\">{}</meta>",
                    page.id,
                    clock(overlay.milliseconds)
                ));
            }
        }
        if total > 0 {
            line(format!("<meta property=\"media:duration\">{}</meta>", clock(total)));
            line(format!("<meta property=\"media:active-class\">{ACTIVE_CLASS}</meta>"));
        }

        let mut manifest = String::new();
        let mut item = |text: String| {
            manifest.push_str("    ");
            manifest.push_str(&text);
            manifest.push('\n');
        };
        item(
            "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>".to_string(),
        );
        item("<item id=\"css\" href=\"stylesheet.css\" media-type=\"text/css\"/>".to_string());
        item("<item id=\"title\" href=\"title.xhtml\" media-type=\"application/xhtml+xml\"/>".to_string());
        let mut spine = String::from("    <itemref idref=\"title\"/>\n");
        for page in pages {
            let id = &page.id;
            match &page.overlay {
                Some(overlay) => {
                    item(format!(
                        "<item id=\"{id}\" href=\"{id}.xhtml\" media-type=\"application/xhtml+xml\" media-overlay=\"{id}-mo\"/>"
                    ));
                    item(format!("<item id=\"{id}-mo\" href=\"{id}.smil\" media-type=\"application/smil+xml\"/>"));
                    item(format!(
                        "<item id=\"{id}-audio\" href=\"{}\" media-type=\"{}\"/>",
                        xml::attribute(&overlay.audio_href),
                        overlay.media_type
                    ));
                },
                None => item(format!("<item id=\"{id}\" href=\"{id}.xhtml\" media-type=\"application/xhtml+xml\"/>")),
            }
            writeln!(spine, "    <itemref idref=\"{id}\"/>").expect("writing to a String");
        }
        Ok(format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" xml:lang=\"{lang}\">\n  <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n{metadata}  </metadata>\n  <manifest>\n{manifest}  </manifest>\n  <spine>\n{spine}  </spine>\n</package>\n",
            lang = xml::attribute(language),
        ))
    }
}

const CONTAINER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n  <rootfiles>\n    <rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\n  </rootfiles>\n</container>\n";

fn page(language: &str, title: &str, body: &str) -> String {
    let lang = xml::attribute(language);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"{lang}\" lang=\"{lang}\">\n<head>\n<meta charset=\"UTF-8\"/>\n<title>{}</title>\n<link rel=\"stylesheet\" type=\"text/css\" href=\"stylesheet.css\"/>\n</head>\n<body>\n{body}\n</body>\n</html>\n",
        xml::text(title)
    )
}

fn nav(language: &str, pages: &[Page]) -> String {
    let mut items = String::from("<li><a href=\"title.xhtml\">Title Page</a></li>\n");
    for page in pages {
        writeln!(items, "<li><a href=\"{}.xhtml\">{}</a></li>", page.id, xml::text(&page.label))
            .expect("writing to a String");
    }
    self::page(
        language,
        "Contents",
        &format!("<nav epub:type=\"toc\" id=\"toc\">\n<h1>Contents</h1>\n<ol>\n{items}</ol>\n</nav>"),
    )
}

fn overlay(id: &str, chapter: &Chapter<'_>, sync: SyncLevel) -> Overlay {
    let ids = body::sentence_ids(&chapter.section.script, id);
    let word_ids = body::word_ids(&chapter.section.script, &ids);
    let audio_href = format!("audio/{id}.{}", chapter.encoded.extension);
    let milliseconds = milliseconds_of(chapter.encoded.samples);
    let mut pars = String::new();
    let mut number = 0;
    let mut par = |pars: &mut String, indent: &str, target: &str, unit: &SyncUnit| {
        number += 1;
        let begin = milliseconds_of(unit.begin);
        let end = milliseconds_of(unit.end).min(milliseconds);
        writeln!(
            pars,
            "{indent}<par id=\"{id}-p{number:04}\">\n{indent}  <text src=\"{id}.xhtml#{target}\"/>\n{indent}  <audio src=\"{}\" clipBegin=\"{}\" clipEnd=\"{}\"/>\n{indent}</par>",
            xml::attribute(&audio_href),
            seconds(begin),
            seconds(end)
        )
        .expect("writing to a String");
    };
    let mut words = chapter.words.iter().peekable();
    for (index, unit) in chapter.units.iter().enumerate() {
        let target = match unit.target {
            UnitRef::Heading => format!("{id}-h"),
            UnitRef::Sentence { block, sentence } | UnitRef::Word { block, sentence, .. } => {
                ids[&(block, sentence)].clone()
            },
        };
        let mut sentence_words = Vec::new();
        if let (SyncLevel::Word, UnitRef::Sentence { block, sentence }) = (sync, unit.target) {
            while let Some(word) = words.next_if(|word| {
                matches!(word.target, UnitRef::Word { block: b, sentence: s, .. } if (b, s) == (block, sentence))
            }) {
                sentence_words.push(word);
            }
        }
        if sentence_words.is_empty() {
            par(&mut pars, "    ", &target, unit);
            continue;
        }
        writeln!(pars, "    <seq id=\"{id}-q{:04}\" epub:textref=\"{id}.xhtml#{target}\">", index + 1)
            .expect("writing to a String");
        for word in sentence_words {
            let UnitRef::Word { block, sentence, word: index } = word.target else {
                continue;
            };
            par(&mut pars, "      ", &word_ids[&(block, sentence, index)], word);
        }
        pars.push_str("    </seq>\n");
    }
    let smil = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<smil xmlns=\"http://www.w3.org/ns/SMIL\" xmlns:epub=\"http://www.idpf.org/2007/ops\" version=\"3.0\">\n<body>\n  <seq id=\"{id}-seq\" epub:textref=\"{id}.xhtml\" epub:type=\"chapter\">\n{pars}  </seq>\n</body>\n</smil>\n"
    );
    Overlay {
        smil,
        audio: chapter.encoded.bytes.clone(),
        audio_href,
        media_type: chapter.encoded.media_type,
        milliseconds,
    }
}

/// Samples to whole milliseconds, rounded down: back-to-back clips stay back
/// to back, and no clip ends after the audio.
fn milliseconds_of(samples: u64) -> u64 {
    samples * 1_000 / u64::from(SAMPLE_RATE)
}

/// SMIL clip value: seconds with three decimals, for example `12.340s`.
fn seconds(milliseconds: u64) -> String {
    format!("{}.{:03}s", milliseconds / 1_000, milliseconds % 1_000)
}

/// `media:duration` value: `h:mm:ss.fff`.
fn clock(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    format!("{}:{:02}:{:02}.{:03}", seconds / 3_600, seconds / 60 % 60, seconds % 60, milliseconds % 1_000)
}
