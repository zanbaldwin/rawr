# rawr-audiobook

Audiobook construction. The crate turns a `rawr_source::Document` into an EPUB 3 with Media Overlays. Only the `script` module exists at the moment. The plan is in `EPUB3-PLAN.md` at the workspace root (untracked).

## `script`

`Script::new(section, heading, options)` builds the spoken form of one section:

- `blocks`: the body in display order (`Block::Paragraph` or `Block::SceneBreak`). The EPUB text uses these.
- `batches`: the heading and body in narration order. One `NarrationBatch` is one text-to-speech request.
- `seed`: the CRC32 of the heading and paragraph texts, joined by `\n`. It changes exactly when the spoken text changes.

Rules (taken from purr, unchanged):

- `p`, `div`, `blockquote`, `li` and `h1`–`h6` end a paragraph. `<hr>` is a scene break. `<br>` is a space.
- A scene break is a paragraph made only of break symbols (`*~-=_#•·◇◆○●+—–※`) and whitespace, or an `align="center"` paragraph with no letters (for example `2.`). Consecutive breaks collapse to one. Breaks at the start and end are removed.
- The cleaning applies NFC, collapses whitespace and trims. scraper decodes all HTML entities.
- Sentences follow UAX #29. A piece that starts with a lowercase letter joins the sentence before it (`“Why?” she asks.`).
- Batches join paragraphs with one `\n`, because whitespace is billed. The heading is always its own batch. A scene break closes a batch. A paragraph longer than `max_batch_chars` is its own batch.
- Summaries, notes and end notes are never spoken. Do not add an option for them, because they are addenda, not content.

## Invariants to keep

- **Change a rule, bump `SCRIPT_RECIPE`.** Any change to the output text changes seeds and batch keys. The recipe version stops old recordings from being mistaken for current ones.
- **The script is provider-neutral.** Provider-specific cleaning (for example ElevenLabs and `[`/`]`) belongs to the provider. That cleaning must keep the character count. `ParagraphRef` offsets and `Paragraph::sentence_chars()` count characters, because provider alignment is per character.
- **The caller gives the spoken heading.** It must match the displayed heading. AO3 chapter titles from rawr-extract already include the "Chapter N: " prefix.

## Tests

- Tests are in `tests/`, not in the source files. They are git-ignored, and they are kept with the fixtures in the `rawr-fixtures` repository.
- `fixture_snapshot` is an `insta` snapshot per fixture (`tests/snapshots/`). Each batch shows its boundary and character count. Sentences are split by ` ‖ `. A rule change shows up as a snapshot diff. Review the diff with `cargo insta review`, or accept it with `INSTA_UPDATE=always`.
- `fixture_invariants` checks every fixture section. It checks offsets, order, the batch size limit, sentence coverage, and the seed.
- Small rule cases build `Section` values in the test file.

## Checks

Run in the `rawr` toolbox:

```sh
cargo test -p rawr-audiobook
cargo clippy -p rawr-audiobook --all-targets
cargo fmt -p rawr-audiobook --check
```
