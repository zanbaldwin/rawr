# rawr-audiobook

Audiobook construction. The crate turns a `rawr_source::Document` into an EPUB 3 with Media Overlays. The modules so far are `script`, `store`, `plan`, `narrate` and `audio`. The plan is in `EPUB3-PLAN.md` at the workspace root (untracked).

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

## `store`

Every paid text-to-speech response is a **recording**, stored under its **batch key** on any `StorageBackend`:

```text
keys/ab/<key>/audio.opus       provider audio, exactly as received
keys/ab/<key>/recording.json   written last: the recording exists only if this exists
ledgers/<blake3(identifier)>.json
```

- `BatchKey::new(KeyInputs)` is BLAKE3 over a canonical text: `KEY_RECIPE`, `SCRIPT_RECIPE`, provider name, fingerprint, language, seed and prepared text. Continuity is not part of the key. Bump `KEY_RECIPE` whenever the canonical text changes.
- Recordings are content-addressed, so resuming needs no state: a batch is paid for exactly when its key is missing.
- `Store::write` writes the audio, then the metadata to `recording.json.tmp`, then renames it into place. Audio without metadata counts as "not recorded". `Store::delete` removes the metadata first for the same reason.
- `RecordingMeta` is this crate's own serde form. `rawr-tts` stays serde-free. Times are RFC 3339 (`time::OffsetDateTime`, UTC).
- `RecordingStatus::AlignmentMismatch` keeps paid audio that cannot be used. It is never paid for again without an explicit flag.
- A **ledger** lists the keys of a document's latest plan. Nothing reads it to resume. `delete_document` and `orphans` use it, and `delete_document` keeps keys that another ledger still uses.

## `plan`

`Plan::new(document, synthesiser, store)` scripts every section, prepares each batch's text with the provider, computes its batch key, and looks the key up in the store. It pays for nothing and writes nothing.

- The fingerprint is checked first, so a missing voice fails the plan (`ErrorKind::Provider`, with the provider's `NoVoice` as the source) before anything else.
- `BatchState`: `Missing` (to be paid for), `Recorded`, or `Mismatched` (paid for, but the alignment did not match; never paid for again without an explicit flag).
- Sections with nothing to speak are listed in `skipped`.
- The spoken heading is `section.title` until the EPUB writer fixes the heading rule.
- `Plan::ledger()` builds the ledger. Narration writes it before the first paid request, so `--estimate` stays read-only. `source_hash` is set by the caller (AO3: `Version::hash`).
- `missing_chars()` is the narration budget's unit: characters, not cost, because provider prices change.

## `narrate`

`narrate(plan, synthesiser, store, options)` returns a `Stream` of `NarrationEvent`s. It pays for missing batches only. `Finished` is always the last event.

- **Order:** the ledger is written first. Sections run at the same time, up to the concurrency (option, else the provider's default). The batches of one section run in order.
- **Write order:** a recording is stored before the section's next request is sent. A store failure stops the whole run, so nothing more is paid for while recordings cannot be kept.
- **Re-check:** each batch is looked up in the store again just before it is paid for, so a stale plan never pays twice.
- **Continuity:** the previous request IDs come from the section's earlier recordings, including recordings from earlier runs, within the provider's age and count limits. The next text is the next batch's prepared text.
- **Errors:** retryable errors are retried with doubling delays. `QuotaExceeded` and `Unauthorised` stop the run. Other errors fail only their section.
- **Mismatch:** the recording is stored with status `alignment_mismatch`, a `Mismatched` event is sent, and the section stops. Later runs skip the batch. Only `retry_mismatched` pays for it again.
- **Budget:** `max_chars` is a character limit for the run. Characters are reserved before each attempt and released when the attempt fails. Real costs from receipts are only reported (`spent`).
- **Runtime:** the stream drives itself through a channel and needs no task spawning. Retry delays use `tokio::time::sleep`.

## `audio`

- `Pcm` is mono `i16`. The pipeline works at `SAMPLE_RATE` (48 kHz) only.
- `fade` applies a linear fade at each edge (the timeline uses 240 samples, 5 ms). The first and last samples become 0.
- `TrailingSound::default()` uses purr's values: at least 100 ms below −60 dBFS, then a 10 ms window above −50 dBFS. `find` returns the start of that silence, searching from the end of the last letter or digit. It cuts the stray sound that long Korean ElevenLabs batches can end with.

## Invariants to keep

- **Change a rule, bump `SCRIPT_RECIPE`.** Any change to the output text changes seeds and batch keys. The recipe version stops old recordings from being mistaken for current ones.
- **The script is provider-neutral.** Provider-specific cleaning (for example ElevenLabs and `[`/`]`) belongs to the provider. That cleaning must keep the character count. `ParagraphRef` offsets and `Paragraph::sentence_chars()` count characters, because provider alignment is per character.
- **The caller gives the spoken heading.** It must match the displayed heading. AO3 chapter titles from rawr-extract already include the "Chapter N: " prefix.

## Tests

- Tests are in `tests/`, not in the source files. They are git-ignored, and they are kept with the fixtures in the `rawr-fixtures` repository.
- `fixture_snapshot` is an `insta` snapshot per fixture (`tests/snapshots/`). Each batch shows its boundary and character count. Sentences are split by ` ‖ `. A rule change shows up as a snapshot diff. Review the diff with `cargo insta review`, or accept it with `INSTA_UPDATE=always`.
- `tests/store.rs` runs against `MockBackend`, plus one round trip on `LocalBackend` in a temporary directory (renames and directory creation on a real filesystem).
- `tests/plan.rs` plans every fixture with the fake provider. It checks that a changed section gets new keys and the other sections keep theirs.
- `tests/narrate.rs` uses a scripted wrapper around the fake provider (unique request IDs, injected errors and mismatches, a request log). It uses small synthetic documents, because the fake makes real PCM (about 5.8 KB for each character).
- `tests/audio.rs` uses synthetic tones and silences.
- `fixture_invariants` checks every fixture section. It checks offsets, order, the batch size limit, sentence coverage, and the seed.
- Small rule cases build `Section` values in the test file.

## Checks

Run in the `rawr` toolbox:

```sh
cargo test -p rawr-audiobook
cargo clippy -p rawr-audiobook --all-targets
cargo fmt -p rawr-audiobook --check
```
