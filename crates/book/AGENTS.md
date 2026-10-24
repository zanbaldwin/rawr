# rawr-book

A separate binary that narrates structured YAML books (`rawr_source::BookYaml`) into EPUB 3 audiobooks. It uses only `rawr-source`, `rawr-tts`, `rawr-audiobook`, `rawr-storage` and `rawr-render`. It does not use `rawr-app`, the cache, the library or rawr's config file.

## Commands

- `rawr-book narrate <book.yaml>` plans the book, prints an estimate (batches, characters, voices), and asks before it pays (`-y` skips the question). `--approve` is needed for ratings the provider restricts. A book without `rating` is `Unrated`, which ElevenLabs restricts. The first Ctrl-C stops new requests and keeps requests in flight. The second one quits at once.
- `rawr-book export <book.yaml> -o out.epub` builds the EPUB from stored recordings and pays nothing. It stops with a list of chapters that are not fully recorded. `--chapters` exports only some chapters as audio, and the others become text-only pages.
- Shared flags: `--language`, `--chapters 1,3-5`, `--provider elevenlabs|fake`, `--recordings DIR`.
- Export flags: `--translation en` (the translated edition), `--sync word|sentence`, `--bitrate` (default 32 kb/s), `--css FILE` (repeatable, applied last).

## Settings and defaults

- **Language:** `--language`, else the book's `language`. Without either, the command stops.
- **Highlight:** `--sync`, else the book's `highlight`, else `sentence`. An unknown `highlight` value stops the export.
- **Voices:** the book's `voices`. The voice with `name: ~` is the narrator (`ElevenLabsSettings::voices[language]`). Every named voice is a role (`ElevenLabsSettings::roles`). Several names can share one voice ID. ElevenLabs without a narrator voice stops the command.
- **API key:** `ELEVENLABS_API_KEY`, needed to narrate only. Export sends nothing.
- **Recordings:** `--recordings`, else rawr's own directory (`ProjectDirs` data directory + `recordings`, for example `~/.local/share/rawr/recordings`), so rawr and rawr-book share one store. The fake provider uses `$TMPDIR/rawr-book-fake`, so test recordings never mix with paid ones.
- **Plan options:** always `PlanOptions { spoken_headings: false, markers: true }`. Headings are shown but never spoken.

## Editions

- The plain and translated editions have the same spoken text and batch keys. Narrate once and export both.
- The translated edition gets the `dc:identifier` `{identifier}:{translation}` (`ExportOptions::identifier`) and the title `{title} ({language}/{translation})`, so reading apps keep the two books apart. The ledger is still found by the plan's identifier.

## Stylesheet

The book gets the builtin `epub.css`, then `BOOK_CSS`, then `SENTENCE_CSS` or `WORD_CSS`, then each `--css` file.

- **`BOOK_CSS`** is the same for both editions. Each sentence is on its own line, with its translation (if any) underneath. A spoken turn (`p:has(> .speaker)`) is a two-column grid: the name (and its translation) in a 5em column on the left, and the sentences on the right with a bar down their left side. Narration has no bar and no name. Directions are italic and slightly faint. `word-break: keep-all` breaks Korean only between words.
- **Ruby as blocks.** The translated edition uses `<ruby>` markup, but the CSS lays each ruby out as two stacked blocks (`display: flex`). WebKit (Apple Books) keeps a ruby base on one unbreakable line, so whole-sentence ruby overflows the page. `white-space: normal` on the ruby and its base fixes this. The same approach is in the Janky Korean deck's `notetypes/assets/style.css`.
- **Padding, not margins.** The space between turns and paragraphs is `padding-bottom`, because Readest replaces a book's paragraph margins with its own. The turn bar is a border on each sentence, with padding between sentences, so the bar stays continuous.
- **Highlights.** `SENTENCE_CSS` highlights the active sentence. `WORD_CSS` highlights the sentence through `.s:has(.-epub-media-overlay-active)` and underlines the active word. Readest uses its own highlight on the active element, so it shows only the word.

## Known behaviour in reading apps

- Apple Books stretches the spaces in wrapped lines when its own "Justify Text" setting is on. The book cannot override that setting.
- `:has()` is needed for the turn grid and the word-mode sentence highlight. Apple Books and Readest support it. In an app without it, turns fall back to plain paragraphs and word mode shows only the word.

## Checks

Run in the `rawr` toolbox:

```sh
cargo build -p rawr-book --release
cargo clippy -p rawr-book --all-targets
cargo fmt -p rawr-book --check
```

There are no unit tests in this crate. The library crates test the pipeline. For an end-to-end check without cost, narrate and export a chapter with `--provider fake --approve`, and open the EPUB in a reading app. The fake audio is a tone for each character, but the highlight moves as it would with real speech.
