use clap::{Args, Parser, Subcommand, ValueEnum};
use directories::ProjectDirs;
use futures::StreamExt;
use rawr_audiobook::audio::LibOpus;
use rawr_audiobook::epub3::SyncLevel;
use rawr_audiobook::narrate::{NarrationEvent, NarrationOptions, StopReason, narrate};
use rawr_audiobook::plan::{BatchState, Plan, PlanOptions};
use rawr_audiobook::store::Store;
use rawr_audiobook::{ExportOptions, export_with_options};
use rawr_render::StyleConfig;
use rawr_source::{BookSettings, BookYaml, Document, Source};
use rawr_storage::BackendHandle;
use rawr_storage::backend::LocalBackend;
use rawr_tts::elevenlabs::{ApiKey, ElevenLabs, ElevenLabsSettings};
use rawr_tts::{Cost, CostUnit, FakeSynthesiser, SynthesiserHandle};
use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const ELEVENLABS_API_KEY_ENV: &str = "ELEVENLABS_API_KEY";

/// Book narration never speaks chapter headings, and always reads the source's markers.
const PLAN_OPTIONS: PlanOptions = PlanOptions { spoken_headings: false, markers: true };

/// The layout of every edition: one sentence per line (its translation, if
/// any, underneath), and each spoken turn as a block with the speaker's name
/// in a column to the left of a bar.
const BOOK_CSS: &str = r#"
p { text-align: start; text-indent: 0; word-break: keep-all; margin: 0; padding-bottom: 1.5em; }
p.direction { font-style: italic; opacity: 0.8; }

/* One sentence per line. */
p > [data-rawr-sentence] { display: block; margin-bottom: 0.35em; }
p > ruby { display: flex; flex-direction: column; align-items: flex-start; margin-bottom: 0.35em; }
p > :is([data-rawr-sentence], ruby):last-child { margin-bottom: 0; }

/* Translations: ruby laid out as two stacked blocks. WebKit keeps a ruby
   base on one unbreakable line, and spreads its words when it wraps, unless
   told otherwise. */
p > ruby, p > ruby > span { white-space: normal; max-width: 100%; }
p > ruby > span { display: block; text-align: left; text-justify: none; ruby-align: start; }
p > ruby > rt { display: block; font-size: 0.62em; line-height: 1.25; opacity: 0.65; white-space: normal; }

/* A spoken turn: a grid with the name in a left column and its sentences
   in the right one, beside a bar. The turn grows to fit a long name, and the
   space between turns is padding, which reading apps leave alone when they
   impose their own paragraph margins. */
p:has(> .speaker) { display: grid; grid-template-columns: 5em minmax(0, 1fr); margin: 0; padding-bottom: 1.5em; }
p:has(> .speaker) > :not(.speaker) {
  grid-column: 2; margin: 0; padding: 0 0 0.35em 0.8em;
  border-left: 0.15em solid rgba(128, 128, 128, 0.45);
}
p:has(> .speaker) > :not(.speaker):last-child { padding-bottom: 0; }
.speaker {
  grid-column: 1; grid-row: 1 / span 99; padding: 0.2em 0.7em 0 0;
  text-align: right; font-size: 0.85em; font-weight: bold; line-height: 1.3; opacity: 0.85;
}
.speaker ruby { display: flex; flex-direction: column; align-items: flex-end; white-space: normal; }
.speaker rt { display: block; font-size: 0.73em; font-weight: normal; line-height: 1.25; opacity: 0.76; white-space: normal; }
"#;

/// The highlight while a sentence is read.
const SENTENCE_CSS: &str = "\
.-epub-media-overlay-active { background-color: rgba(255, 213, 0, 0.35); border-radius: 0.2em; }
";

/// The sentence being read, and a subtler mark on the word being read.
const WORD_CSS: &str = "\
.s:has(.-epub-media-overlay-active) { background-color: rgba(255, 213, 0, 0.3); border-radius: 0.2em; }
.w.-epub-media-overlay-active { text-decoration: underline 0.12em rgba(214, 120, 0, 0.9); text-underline-offset: 0.18em; }
";

#[derive(Debug, Parser)]
#[command(
    name = "rawr-book",
    version,
    about = "Narrate structured YAML books into EPUB 3 audiobooks"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Pay for the recordings a book still needs. Shows an estimate and asks first.
    Narrate(NarrateArgs),
    /// Build the audiobook from recordings that already exist. Pays nothing.
    Export(ExportArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Provider {
    /// ElevenLabs, with the voices the book lists. Needs `ELEVENLABS_API_KEY` to narrate.
    Elevenlabs,
    /// Free tones with exact timing, for checking layout and highlighting in reading apps.
    Fake,
}

#[derive(Debug, Args)]
struct BookArgs {
    /// The book (YAML).
    book: PathBuf,
    /// The language that is shown and spoken. Default: the book's `language`.
    #[arg(long, value_name = "CODE")]
    language: Option<String>,
    /// Only these chapters, for example `1,3-5`. Default: all.
    #[arg(long, value_name = "LIST", value_parser = parse_chapters)]
    chapters: Option<Chapters>,
    /// The text-to-speech provider.
    #[arg(long, value_enum, default_value_t = Provider::Elevenlabs)]
    provider: Provider,
    /// Where recordings are kept. Default: rawr's recordings directory
    /// (a temporary directory for the fake provider).
    #[arg(long, value_name = "DIR")]
    recordings: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct NarrateArgs {
    #[command(flatten)]
    book: BookArgs,
    /// Approve sending a book whose rating the provider restricts (for ElevenLabs: Explicit and Not Rated).
    #[arg(long)]
    approve: bool,
    /// Do not ask for confirmation before paying.
    #[arg(short = 'y', long)]
    yes: bool,
    /// Stop before sending more than this many characters.
    #[arg(long, value_name = "N")]
    max_chars: Option<usize>,
    /// Pay again for batches whose recording had an alignment mismatch.
    #[arg(long)]
    retry_mismatched: bool,
    /// Requests at the same time (default: the provider's).
    #[arg(long, value_name = "N")]
    concurrency: Option<usize>,
}

#[derive(Debug, Args)]
struct ExportArgs {
    #[command(flatten)]
    book: BookArgs,
    /// Where to write the EPUB.
    #[arg(short, long, value_name = "FILE")]
    output: PathBuf,
    /// Show this language as ruby over the text, for example `en`. Default: no translation.
    #[arg(long, value_name = "CODE")]
    translation: Option<String>,
    /// What to highlight: `sentence` or `word`. Default: the book's `highlight`, else `sentence`.
    #[arg(long, value_name = "LEVEL", value_parser = parse_sync)]
    sync: Option<SyncLevel>,
    /// Opus bitrate of the audio, in kb/s.
    #[arg(long, value_name = "KBPS", default_value_t = 32)]
    bitrate: u32,
    /// Extra stylesheets, applied after the built-in ones.
    #[arg(long, value_name = "FILE")]
    css: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct Chapters(Vec<u32>);

fn parse_chapters(value: &str) -> std::result::Result<Chapters, String> {
    let mut chapters = Vec::new();
    for part in value.split(',').map(str::trim).filter(|part| !part.is_empty()) {
        let number = |text: &str| text.trim().parse::<u32>().map_err(|_| format!("not a chapter number: {text}"));
        match part.split_once('-') {
            Some((first, last)) => chapters.extend(number(first)?..=number(last)?),
            None => chapters.push(number(part)?),
        }
    }
    if chapters.is_empty() {
        return Err("no chapters".to_string());
    }
    Ok(Chapters(chapters))
}

fn parse_sync(value: &str) -> std::result::Result<SyncLevel, String> {
    value.parse()
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Narrate(args) => narrate_book(args).await,
        Command::Export(args) => export_book(args).await,
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        },
    }
}

/// An error and its first cause, and theirs, as one line.
fn describe(frame: &exn::Frame) -> String {
    match frame.children().first() {
        Some(child) => format!("{}: {}", frame.error(), describe(child)),
        None => frame.error().to_string(),
    }
}

trait Described<T> {
    fn described(self) -> Result<T>;
}

impl<T, E> Described<T> for std::result::Result<T, exn::Exn<E>>
where
    E: Error + Send + Sync + 'static,
{
    fn described(self) -> Result<T> {
        self.map_err(|error| describe(error.frame()).into())
    }
}

/// The book's bytes and settings, and the language to use.
struct Book {
    input: Vec<u8>,
    settings: BookSettings,
    language: String,
}

impl Book {
    fn load(args: &BookArgs) -> Result<Self> {
        let input = std::fs::read(&args.book).map_err(|error| format!("{}: {error}", args.book.display()))?;
        let settings = BookYaml::settings(&input).described()?;
        let language = args
            .language
            .clone()
            .or_else(|| settings.language.clone())
            .ok_or("the book has no `language`: pass --language")?;
        Ok(Self { input, settings, language })
    }

    fn document(&self, translation: Option<&str>) -> Result<Document> {
        let source = match translation {
            Some(translation) => BookYaml::new(&self.language).with_translation(translation),
            None => BookYaml::new(&self.language),
        };
        source.to_document(&self.input).described()
    }

    /// The synthesiser. `need_key` is false for export, which sends nothing.
    fn synthesiser(&self, provider: Provider, need_key: bool) -> Result<SynthesiserHandle> {
        if provider == Provider::Fake {
            return Ok(Arc::new(FakeSynthesiser::new("fake")));
        }
        let key = std::env::var(ELEVENLABS_API_KEY_ENV).unwrap_or_default();
        if need_key && key.is_empty() {
            return Err(format!("{ELEVENLABS_API_KEY_ENV} is not set").into());
        }
        let narrator = self
            .settings
            .voices
            .iter()
            .find(|voice| voice.name.is_none())
            .ok_or("the book lists no narrator voice (a voice with `name: ~`)")?;
        let mut settings = ElevenLabsSettings::new(
            ApiKey::new(key),
            BTreeMap::from([(self.language.clone(), narrator.voice.clone())]),
        );
        settings.roles = self
            .settings
            .voices
            .iter()
            .filter_map(|voice| voice.name.clone().map(|name| (name, voice.voice.clone())))
            .collect();
        Ok(Arc::new(ElevenLabs::new(settings).described()?))
    }
}

fn store(args: &BookArgs) -> Result<Store> {
    let directory = match (&args.recordings, args.provider) {
        (Some(directory), _) => directory.clone(),
        (None, Provider::Fake) => std::env::temp_dir().join("rawr-book-fake"),
        (None, Provider::Elevenlabs) => ProjectDirs::from("", "", "rawr")
            .map(|dirs| dirs.data_dir().join("recordings"))
            .ok_or("no data directory: pass --recordings")?,
    };
    let backend: BackendHandle =
        Arc::new(LocalBackend::new("recordings", directory.to_string_lossy(), true).described()?);
    Ok(Store::new(backend))
}

fn included(chapters: Option<&Chapters>, number: u32) -> bool {
    chapters.is_none_or(|Chapters(chapters)| chapters.contains(&number))
}

fn cost_text(cost: &Cost) -> String {
    match cost.unit {
        CostUnit::Credits => format!("{} credits", cost.amount),
        CostUnit::Characters => format!("{} characters", cost.amount),
        CostUnit::Free => "nothing".to_string(),
    }
}

async fn narrate_book(args: NarrateArgs) -> Result<ExitCode> {
    let book = Book::load(&args.book)?;
    let document = book.document(None)?;
    let synthesiser = book.synthesiser(args.book.provider, true)?;
    let store = store(&args.book)?;
    let plan = Plan::with_options(&document, synthesiser.as_ref(), &store, &PLAN_OPTIONS).await.described()?;
    let chapters = args.book.chapters.as_ref();

    let to_pay: Vec<_> = plan
        .sections
        .iter()
        .filter(|section| included(chapters, section.script.section))
        .flat_map(|section| &section.batches)
        .filter(|batch| match batch.state {
            BatchState::Missing => true,
            BatchState::Mismatched => args.retry_mismatched,
            BatchState::Recorded => false,
        })
        .collect();
    let chars: usize = to_pay.iter().map(|batch| batch.chars).sum();
    let roles = plan.roles();
    println!(
        "\"{}\" · {} · {} section(s) · {} batches: {} recorded, {} to record ({chars} characters)",
        plan.title,
        plan.language.as_deref().unwrap_or("unknown language"),
        plan.sections.len(),
        plan.batches().count(),
        plan.count(BatchState::Recorded),
        to_pay.len(),
    );
    if !roles.is_empty() {
        println!("Voices: narrator, {}", roles.join(", "));
    }
    if plan.needs_approval && !args.approve {
        eprintln!("The provider restricts this book's rating ({:?}): pass --approve to send it.", plan.rating);
        return Ok(ExitCode::FAILURE);
    }
    if chars == 0 {
        println!("Nothing to record.");
        return Ok(ExitCode::SUCCESS);
    }
    if !args.yes && !confirm(&format!("Send {chars} characters to {}?", plan.provider))? {
        eprintln!("Nothing was sent.");
        return Ok(ExitCode::FAILURE);
    }

    let stop = CancellationToken::new();
    let listener = {
        let stop = stop.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                stop.cancel();
                eprintln!("Stopping: requests in flight finish and are kept. Press Ctrl-C again to quit at once.");
                if tokio::signal::ctrl_c().await.is_ok() {
                    std::process::exit(130);
                }
            }
        })
    };
    let options = NarrationOptions {
        concurrency: args.concurrency,
        max_chars: args.max_chars,
        retry_mismatched: args.retry_mismatched,
        approved: args.approve,
        stop: Some(stop),
        sections: chapters.map(|Chapters(chapters)| chapters.clone()),
        ..NarrationOptions::default()
    };
    let mut failed = false;
    let mut events = narrate(&plan, Arc::clone(&synthesiser), &store, options);
    while let Some(event) = events.next().await {
        match event {
            NarrationEvent::Started { .. } => {},
            NarrationEvent::Recorded { section, batch, chars, .. } => {
                println!("Chapter {section}, batch {}: recorded ({chars} characters)", batch + 1);
            },
            NarrationEvent::Retrying { section, batch, attempt, delay, error } => {
                eprintln!("Chapter {section}, batch {}: {error}. Retry {attempt} in {} s.", batch + 1, delay.as_secs())
            },
            NarrationEvent::Mismatched { section, batch, .. } => {
                failed = true;
                eprintln!(
                    "Chapter {section}, batch {}: the alignment does not match the text. The recording is kept; --retry-mismatched pays for it again.",
                    batch + 1
                );
            },
            NarrationEvent::SectionFailed { section, batch, error } => {
                failed = true;
                eprintln!("Chapter {section}, batch {}: {error}. The other chapters continue.", batch + 1);
            },
            NarrationEvent::Stopped { reason } => {
                failed = true;
                let reason = match reason {
                    StopReason::NeedsApproval(rating) => format!("the rating ({rating:?}) needs --approve"),
                    StopReason::Budget => "the --max-chars limit is reached".to_string(),
                    StopReason::Provider(error) => error.to_string(),
                    StopReason::Cancelled => "cancelled".to_string(),
                    StopReason::Storage => "a recording could not be stored".to_string(),
                };
                eprintln!("Stopped: {reason}.");
            },
            NarrationEvent::Finished { chars, spent } => {
                let spent: Vec<String> = spent.iter().map(cost_text).collect();
                let spent = if spent.is_empty() { String::new() } else { format!(" · spent {}", spent.join(", ")) };
                println!("Sent {chars} characters{spent}.");
            },
        }
    }
    listener.abort();
    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

fn confirm(prompt: &str) -> Result<bool> {
    eprint!("{prompt} [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}

async fn export_book(args: ExportArgs) -> Result<ExitCode> {
    let book = Book::load(&args.book)?;
    let sync = match (args.sync, &book.settings.highlight) {
        (Some(sync), _) => sync,
        (None, Some(highlight)) => highlight.parse().map_err(|error| format!("the book's `highlight`: {error}"))?,
        (None, None) => SyncLevel::default(),
    };
    let mut document = book.document(args.translation.as_deref())?;
    let synthesiser = book.synthesiser(args.book.provider, false)?;
    let store = store(&args.book)?;
    let plan = Plan::with_options(&document, synthesiser.as_ref(), &store, &PLAN_OPTIONS).await.described()?;
    let chapters = args.book.chapters.as_ref();

    let missing: Vec<String> = plan
        .sections
        .iter()
        .filter(|section| included(chapters, section.script.section))
        .filter(|section| section.batches.iter().any(|batch| batch.state != BatchState::Recorded))
        .map(|section| section.script.section.to_string())
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "chapter(s) {} are not fully recorded. Run `rawr-book narrate`, or export only recorded chapters with --chapters.",
            missing.join(", ")
        )
        .into());
    }

    let identifier = args.translation.as_ref().map(|translation| format!("{}:{translation}", document.meta.identifier));
    if let Some(translation) = &args.translation {
        document.meta.title = format!("{} ({}/{translation})", document.meta.title, book.language);
    }
    let mut styles =
        StyleConfig::new().with_builtin("epub.css").described()?.with_content(BOOK_CSS).with_content(match sync {
            SyncLevel::Sentence => SENTENCE_CSS,
            SyncLevel::Word => WORD_CSS,
        });
    for css in &args.css {
        styles = styles.with_file(css).described()?;
    }
    let options = ExportOptions {
        sync,
        identifier,
        sections: chapters.map(|Chapters(chapters)| chapters.clone()),
    };
    let codec = LibOpus::new(args.bitrate);
    let package = export_with_options(&document, &plan, &store, &codec, &styles, &options).await.described()?;
    let file = std::fs::File::create(&args.output).map_err(|error| format!("{}: {error}", args.output.display()))?;
    package.write(file).described()?;
    println!(
        "Wrote {} ({} narrated chapter(s), {sync} highlighting{})",
        args.output.display(),
        package.chapters.len(),
        args.translation.as_ref().map(|translation| format!(", {translation} ruby")).unwrap_or_default()
    );
    Ok(ExitCode::SUCCESS)
}
