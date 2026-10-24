# rawr-source

Content input for narration. A `Source` turns decompressed input bytes into a `Document`. The rest of the audiobook pipeline reads only `DocumentMeta` and `Section`.

## Model

- `Document`: `meta`, `sections`, `origin`.
- `Section` has the same shape as `rawr_extract::models::ChapterContent`. `body_xhtml`, `summary`, `notes` and `end_notes` are XHTML fragments, not whole documents.
- `DocumentMeta.summary` is plain text, not XHTML.
- `DocumentMeta.rating` is a source-neutral `Rating`: `General`, `Teen`, `Mature`, `Explicit` or `Unrated`. AO3 "Not Rated" and a missing rating are both `Unrated`. Sources state the rating only. Each TTS provider decides which ratings need approval (`rawr_tts::Capabilities::needs_approval`).
- `Document::source_hash()` gives the BLAKE3 hash (hex) of the input, for the narration ledger. AO3 gives `Version::hash`. `Generic` gives `None`.
- `Origin` is `#[non_exhaustive]`. The pipeline must not match on it. Only the edges use it: the rating gate, path templates (`PathGenerator` needs a `Version`), and the AO3 title page.

## Sources

- `Ao3Html` (feature `ao3`) accepts AO3 HTML downloads only. It does not accept AO3 PDF or EPUB downloads, the live site, or rawr's cache database.
- `to_document` parses the input twice. `rawr_extract::extract` parses the header (about 25 KiB) for the `Version`. `Extractor::from_html` parses the full document for the chapters. The cost is small, and rawr-extract stays unchanged.
- `DocumentMeta.identifier` for AO3 is the work URL (`https://archiveofourown.org/works/{id}`). It is a valid EPUB `dc:identifier`.
- `DocumentMeta.language` is `Language::iso_code`. It is `None` when AO3 gives a language name that rawr-extract does not know.

### `BookYaml` (feature `book`)

A structured book in YAML: `chapters` of `paragraphs` of `sentences`. `BookYaml::new("ko")` sets the language that is shown and spoken. `.with_translation("en")` gives the translated edition.

- **Texts:** the book title, summary, chapter titles, `character` and every sentence are a `Text`. A `Text` is a plain string or a map of language code to text with at least one entry. A plain string is the same in every language and never gets a translation. A map without the book's language is an `Extract` error, also for titles and labels. There is no fallback to another language.
- **Optional fields:** `identifier`, `authors`, `summary`, `rating` (`general`, `teen`, `mature`, `explicit`; anything else is `Unrated`), `language`, `highlight`, `voices`. Unknown keys are ignored.
- **`BookYaml::settings(input)`** gives what is not part of the `Document`: `language`, `highlight` (a string, so this crate does not depend on `rawr-audiobook`) and `voices`. A voice with `name: ~` is the narrator.
- **Identifier:** the YAML `identifier`, else `urn:rawr-book:{slug(title)}`. The slug comes from the title in the book's language. Changing that title changes the identifier and orphans the recordings, so give long-lived books an explicit `identifier`. Both editions have the same identifier. The caller gives the translated EPUB its own `dc:identifier`.
- **Body XHTML:** one `<p>` per paragraph, `<p class="direction">` for `direction: true`. The speaker label is `<span class="speaker" data-rawr-speak="never">`. Each sentence is `<span data-rawr-sentence="">`. The paragraph gets `data-rawr-voice="{name}"` only when its `character` matches a named voice in `voices` and it is not a direction. Unvoiced speakers use the narrator and do not split batches.
- **Translated edition:** each sentence and speaker label is wrapped in `<ruby>…<rt data-rawr-speak="never">EN</rt></ruby>`. The pieces are joined with the same single space in both editions, so both editions have the same spoken text, sentences and batch keys.
- **`accepts`** checks for valid UTF-8 and a line that starts with `chapters:`. The YAML crate is `serde-saphyr` (default features off, `deserialize` only). `serde_yaml` is deprecated.

## Pitfalls

- **Summary format and feature unification.** rawr-extract converts the work summary to Markdown when its `markdown` feature is on. This crate does not turn that feature on. But if another crate in the same build turns it on, Cargo unifies the features, and `DocumentMeta.summary` becomes Markdown.

## Tests

- Tests live in `tests/`, not in the source files. They use the AO3 fixtures in the workspace's `tests/fixtures/` (bzip2-compressed HTML downloads).
- `tests/` and the fixtures are git-ignored, because the fixtures are real fics. They are kept in the separate `rawr-fixtures` repository.
- Each fixture test is an `rstest` `#[files(...)]` case. The expected values come from `rawr-extract` itself, so the tests check the mapping, not the fic content.
- `tests/book.rs` uses small inline YAML books. It checks the exact XHTML of both editions, escaping, plain and per-language texts, metadata, settings and errors.

## Checks

Run in the `rawr` toolbox:

```sh
cargo test -p rawr-source --all-features
cargo test -p rawr-source --features ao3
cargo test -p rawr-source --features book
cargo test -p rawr-source
cargo clippy -p rawr-source --all-features --all-targets
cargo fmt -p rawr-source --check
```
