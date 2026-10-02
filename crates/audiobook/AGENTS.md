# rawr-audiobook

Audiobook construction. The crate turns a `rawr_source::Document` into an EPUB 3 with Media Overlays. The modules are `script`, `store`, `plan`, `narrate`, `audio`, `timeline` and `epub3`. The plan is in `EPUB3-PLAN.md` at the workspace root (untracked).

## `script`

`Script::new(section, heading, options)` builds the spoken form of one section:

- `blocks`: the body in reading order (`Block::Paragraph` or `Block::SceneBreak`). Each paragraph has its spoken `text`, its `sentences`, and a `sources` entry per sentence: where the sentence is in the section's XHTML (`SourceRange` of two `TextPosition`s). The EPUB shows the XHTML as-is and uses `sources` to place the sync spans.
- `batches`: the heading and body in narration order. One `NarrationBatch` is one text-to-speech request.
- `seed`: the CRC32 of the heading and paragraph texts, joined by `\n`. It changes exactly when the spoken text changes.

Rules (taken from purr, unchanged):

- `p`, `div`, `blockquote`, `li` and `h1`–`h6` end a paragraph. `<hr>` is a scene break. A `<br>`, with the whitespace around it, is one newline (`SCRIPT_RECIPE` 2; recipe 1 made it a space). Under UAX #29 a newline ends a sentence, so each line is its own sync unit unless the next line starts lowercase.
- A scene break is a paragraph made only of break symbols (`*~-=_#•·◇◆○●+—–※`) and whitespace, or an `align="center"` paragraph with no letters (for example `2.`). Consecutive breaks collapse to one. Breaks at the start and end are removed.
- The cleaning works per grapheme: NFC, whitespace runs to one space, trimmed. NFC never changes grapheme boundaries, so every cleaned character maps back to exactly one source grapheme. scraper decodes all HTML entities. Inline formatting is not kept: the display uses the XHTML itself.
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
- A **ledger** lists the keys of a document's latest plan. It also keeps the voice's display name (`voice_name`) for the narrator credit. Nothing reads it to resume. `delete_document` and `orphans` use it, and `delete_document` keeps keys that another ledger still uses.

## `plan`

`Plan::new(document, synthesiser, store)` scripts every section, prepares each batch's text with the provider, computes its batch key, and looks the key up in the store. It pays for nothing and writes nothing.

- The fingerprint is checked first, so a missing voice fails the plan (`ErrorKind::Provider`, with the provider's `NoVoice` as the source) before anything else.
- `BatchState`: `Missing` (to be paid for), `Recorded`, or `Mismatched` (paid for, but the alignment did not match; never paid for again without an explicit flag).
- Sections with nothing to speak are listed in `skipped`.
- The spoken heading is `rawr_render::pages::chapter_heading()`, the same rule as rawr's text EPUB, so the spoken heading always equals the displayed one: "Chapter N: Title" (an AO3 "Chapter N" prefix is not doubled) in a multi-section document, and **no heading** for a single section.
- `Plan::ledger()` builds the ledger. Narration writes it before the first paid request, so `--estimate` stays read-only. `source_hash` comes from `Document::source_hash()` (AO3: `Version::hash`, the BLAKE3 hash of the HTML).
- `missing_chars()` is the narration budget's unit: characters, not cost, because provider prices change.

## `narrate`

`narrate(plan, synthesiser, store, options)` returns a boxed stream (`BoxStream`) of `NarrationEvent`s, so callers use `next()` without `pin!`. It pays for missing batches only. `Finished` is always the last event.

- **Order:** the ledger is written first, with the voice name from `Synthesiser::voice_name()`. If that free lookup fails, the name from an earlier run is kept and narration continues. Sections run at the same time, up to the concurrency (option, else the provider's default). The batches of one section run in order.
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

### Codec (feature `libopus`)

- `Codec::decode` turns provider audio into 48 kHz mono PCM. For Ogg Opus it drops the OpusHead pre-skip and trims to the last granule position. The result has the same length as ffmpeg's decode. `Pcm` audio passes through at 48 kHz only.
- `Codec::encode` writes Opus in MP4 (`audio/mp4`, `.mp4`): mono, `Application::Audio`, 20 ms frames, `LibOpus::new(bitrate_kbps)` (default 32). The input is padded with zeros to whole frames after the encoder delay (pre-skip = `get_lookahead()`).
- `audio/mp4.rs` is our own writer for this one fixed shape: `ftyp`, `moov` (faststart), `mdat`. It follows ffmpeg's boxes: `Opus` + `dOps`, an `elst` with media time = pre-skip and duration = real length, the last `stts` delta shortened so the media ends at pre-skip + length, and a `roll` group (−4, 80 ms). Offsets and durations are 32-bit. Longer audio gives `UnsupportedAudio`.
- Opus output is about 2 samples (0.04 ms) early against the input, a normal codec phase shift. It is far below the 1 ms sync precision.
- libopus 1.6.1 is built from source (`opusic-sys`, cmake + a C compiler), like `libbzip3-sys`. It builds for `x86_64-unknown-linux-musl`.
- **To do:** add `opus-pure` as a second `Codec`, and test that its decode and encode are effectively the same as libopus (exact length, ±1 LSB decode, similar size and SNR).

## `timeline`

`assemble(section, recordings, codec)` joins one section's recordings (one for each planned batch, in order) into a `ChapterAudio`: the chapter's PCM and its sync map. Export never pays: a missing or mismatched recording gives `MissingRecording`, and a recording without alignment gives `MissingAlignment`.

- **Trim** (from purr): keep from the first character's start − 50 ms to the last character's end + 100 ms, clamped to the decoded length. ElevenLabs' last timestamp can be up to 80 ms past the audio. A `TrailingSound` after the last letter or digit cuts the end at that silence + 50 ms.
- **Fades:** 5 ms (240 samples) at each edge of each recording. No crossfades.
- **Silence before a batch, by `Boundary`:** `SectionStart` 0.5 s (the chapter's lead-in), `Heading` 1.0 s, `Paragraph` 0.45 s, `SceneBreak` 1.75 s. There is 1.0 s of lead-out after the last batch.
- **Trim report** (dev and test builds only, `cfg(debug_assertions)`; tests that read it are gated the same way, so `cargo test --release` still builds): `ChapterAudio::trims` has one `TrimReport` per recording: its key, decoded length, kept range, `EndRule` (`LastTimestamp` or `TrailingSound`), the overshoot of the last timestamp past the audio, and where it starts in the chapter. It is purr's record of cuts (`end_rule`, `cut_seconds_before_file_end`). Use it to find and listen to bad joins. Release builds do not have the field.
- **Sync map:** one `SyncUnit` for the heading and one for each sentence, in samples. A unit begins at the output time of its first character (`ParagraphRef` offset + `Paragraph::sentence_chars()`). Units are back to back: the first begins at 0, so it covers the lead-in, and the last ends at the end of the audio, so it covers the lead-out. Begins are strictly increasing. Readers play only clip ranges, so gaps would remove the pauses.

## `epub3`

`Package { document, chapters, styles, narrator, modified }.write(writer)` writes an EPUB 3.3 with Media Overlays (reflowable). A `Chapter` owns its sync units and encoded audio, but not the PCM. Drop each `ChapterAudio` after its audio is encoded, so only one chapter's PCM is in memory at a time. The crate has its own `zip` dependency (6.0, `deflate`, the same as epub-builder's), because the audiobook can be built without rawr's `epub` feature.

- **Layout:** `mimetype` (first, stored, no extra field), `META-INF/container.xml`, and `OEBPS/` with `content.opf`, `nav.xhtml`, `stylesheet.css` (the `StyleConfig`), `title.xhtml`, then `cNNN.xhtml`, `cNNN.smil` and `audio/cNNN.mp4` for each narrated section. A section with nothing to speak is a text-only page, with no overlay.
- **Text:** `chapter_body()` writes the section's rawr-xhtml output again with a `<span id="cNNN-sNNNN">` around each sentence. Each span sits under the deepest element that holds both ends of its sentence. Inline elements are split only where a sentence crosses them, and they are reopened without their `id`. Start tags are written lazily, so a split never leaves an empty element. The text and the block elements are exactly those of the source. Attributes come out in the parser's order. The spoken heading is `<h2 class="chapter-title"><span id="cNNN-h">`.
- **Overlays:** one `<par>` per `SyncUnit`. Clip times are whole milliseconds, rounded down, written as `12.340s`. Clips are back to back, and the last one ends at the audio's length. `media:duration` (`h:mm:ss.fff`) is given per overlay and in total, and the total is their exact sum.
- **No highlight colour:** `media:active-class` (`-epub-media-overlay-active`) is declared, but the book has no rule for it. Reading apps such as Readest, and user stylesheets, choose the highlight.
- **Title page:** with the `ao3` feature, an AO3 work gets `rawr_render::pages::title_page_body()`, as in rawr's text EPUB. Otherwise a minimal page is built from `DocumentMeta`. Both add "Narrated by …".
- **Narrator:** `narrator_credit(provider, fingerprint, voice_name)` gives "{voice name} ({provider} {model})", for example "Amelia - Enthusiastic and Expressive (ElevenLabs eleven_v4)". The voice name comes from the ledger.
- **Escaping:** `epub3/xml.rs` uses the same rules as rawr-xhtml (`&`, `<`, `>`, the no-break space as `&#160;`, and `"` in attributes), so text written again comes out the same.

## `export`

`rawr_audiobook::export(document, plan, store, codec, styles)` builds the `Package` from stored recordings. It never pays. For each planned section it reads the recordings, calls `assemble`, encodes the audio, and drops the PCM before the next section. A batch with no recording gives `MissingRecording`. It does not use the plan's `BatchState`s, so a plan made before narration still works. The narrator credit uses the ledger's voice name, and `modified` is the current time (tests set it after the call). Decoding and encoding run on the caller's task.

## Invariants to keep

- **`TextPosition::node` counts every text node in document order**, as `scraper::Html::parse_fragment(body_xhtml)` parses the body (whitespace-only nodes included). Anything that uses `sources` must walk the body the same way.

- **Change a rule, bump `SCRIPT_RECIPE`.** Any change to the output text changes seeds and batch keys. The recipe version stops old recordings from being mistaken for current ones.
- **The script is provider-neutral.** Provider-specific cleaning (for example ElevenLabs and `[`/`]`) belongs to the provider. That cleaning must keep the character count. `ParagraphRef` offsets and `Paragraph::sentence_chars()` count characters, because provider alignment is per character.
- **The caller gives the spoken heading.** `Plan::new` uses `chapter_heading()` so it matches the displayed heading. AO3 chapter titles from rawr-extract often already include the "Chapter N: " prefix.

## Tests

- Tests are in `tests/`, not in the source files. They are git-ignored, and they are kept with the fixtures in the `rawr-fixtures` repository.
- `fixture_snapshot` is an `insta` snapshot per fixture (`tests/snapshots/`). Each batch shows its boundary and character count. Sentences are split by ` ‖ `. A rule change shows up as a snapshot diff. Review the diff with `cargo insta review`, or accept it with `INSTA_UPDATE=always`.
- `tests/store.rs` runs against `MockBackend`, plus one round trip on `LocalBackend` in a temporary directory (renames and directory creation on a real filesystem).
- `tests/plan.rs` plans every fixture with the fake provider. It checks that a changed section gets new keys and the other sections keep theirs.
- `tests/narrate.rs` uses a scripted wrapper around the fake provider (unique request IDs, injected errors and mismatches, a request log). It uses small synthetic documents, because the fake makes real PCM (about 5.8 KB for each character).
- `tests/audio.rs` uses synthetic tones and silences.
- `tests/codec.rs` (`--features libopus`): decodes the ElevenLabs fixtures to ffmpeg's exact lengths. It parses our MP4 with `shiguredo_mp4` (dev-dependency) and checks every box against ffmpeg's layout. It decodes the packets again to check length, alignment and SNR. A last test reads the file with `ffprobe`, when installed.
- `tests/timeline.rs` checks exact layouts from fake recordings (the fake gives 60 ms for each character), including the trim report. A stray sound appended after the speech is cut, a plain trailing silence keeps the 100 ms margin, and an overshooting last timestamp is clamped. With `libopus`, it also joins real ElevenLabs audio from the fixtures and checks that only the margins are added and nothing spoken is cut.
- `tests/epub3.rs` checks every fixture section's `chapter_body`: well-formed XML, the same text and block elements as the source, one span per sentence with the right id and text, and no repeated id. A small document narrated by the fake is written as a full package. Its zip layout, links, clip ranges and durations are checked, and its files are `insta` snapshots. With `--features ao3`, the AO3 title page is checked too.
- `fixture_invariants` checks every fixture section. It checks offsets, order, the batch size limit, sentence coverage, and the seed.
- `fixture_sources_point_at_each_sentence` cuts each sentence's source range out of the real text nodes, normalises it, and compares it with the sentence.
- Small rule cases build `Section` values in the test file.

## Checks

Run in the `rawr` toolbox:

```sh
cargo test -p rawr-audiobook --features libopus,ao3
cargo test -p rawr-audiobook
cargo clippy -p rawr-audiobook --features libopus --all-targets
cargo clippy -p rawr-audiobook --all-targets
cargo fmt -p rawr-audiobook --check
```
