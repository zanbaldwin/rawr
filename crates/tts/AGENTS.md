# rawr-tts

Text to speech. `Synthesiser` is the one interface every provider implements. Use it as `SynthesiserHandle` (`Arc<dyn Synthesiser>`), like `StorageBackend`.

## Contract

- `name()` is part of every batch key. Never change a provider's name.
- `prepare(text)` is the provider's own text cleaning. It must keep the character count, because alignment indices are matched against `rawr-audiobook` script offsets. Implementations `debug_assert` this.
- `fingerprint(language)` describes everything provider-specific that changes the audio (model, voice, settings, output format). `Fingerprint::new` sorts the keys. The store puts the fingerprint in the batch key, so recordings from different voices never mix. It gives `NoVoice` for a language with no voice, before anything is paid for.
- `synthesise(request)` sends one request. The text must already be prepared.
- Providers classify errors. Only `RateLimited` and `Unavailable` are retryable. The caller owns retries and back-off.
- `AlignmentMismatch` carries the paid `Synthesis`. The caller must store it before it fails.

## Types

- Times are whole milliseconds (`CharacterTiming`).
- `AudioFormat::Pcm` is signed 16-bit little-endian mono with no header.
- `Receipt.headers` keeps every response header except credentials and cookies. Raw response bodies are not kept. They appear only in `trace`-level logs.

## Providers

- `fake` (feature `fake`): deterministic, no network, no cost. PCM at 48 kHz, 60 ms for each character, a quiet tone for letters and digits, and exact alignment. Its capabilities match ElevenLabs, so the whole pipeline can run against it in tests. `prepare` maps `[`→`(` and `]`→`)` like ElevenLabs.

## Known lint

`clippy::double_must_use` fires on the `#[async_trait]` expansion. `rawr-storage` has the same warning. Leave it until the trait moves to native async.

## Tests

Tests are in `tests/` (git-ignored, kept in `rawr-fixtures`). Run in the `rawr` toolbox:

```sh
cargo test -p rawr-tts --features fake
cargo test -p rawr-tts
cargo clippy -p rawr-tts --all-features --all-targets
cargo fmt -p rawr-tts --check
```
