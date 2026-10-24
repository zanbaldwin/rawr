# rawr-tts

Text to speech. `Synthesiser` is the one interface every provider implements. Use it as `SynthesiserHandle` (`Arc<dyn Synthesiser>`), like `StorageBackend`.

## Contract

- `name()` is part of every batch key. Never change a provider's name.
- `prepare(text)` is the provider's own text cleaning. It must keep the character count, because alignment indices are matched against `rawr-audiobook` script offsets. Implementations `debug_assert` this.
- `fingerprint(language)` describes everything provider-specific that changes the audio (model, voice, settings, output format). `Fingerprint::new` sorts the keys. The store puts the fingerprint in the batch key, so recordings from different voices never mix. It gives `NoVoice` for a language with no voice, before anything is paid for.
- `synthesise(request)` sends one request. The text must already be prepared.
- `voice_name(language)` gives the voice's display name for the narrator credit. It is never paid for. The default returns `None` after checking that a voice exists. The fake returns its voice name. ElevenLabs uses `GET /v1/voices/{voice_id}` (`voice_url`, `parse_voice_name`).
- **Voice roles:** `SynthesisRequest.role` names a voice role, for example a character. `role_fingerprint(language, role)` and `role_voice_name(language, role)` are the role-aware forms. Role `None` gives exactly the same result as `fingerprint` and `voice_name`. The defaults ignore the role, so a provider without roles narrates every role with the language's voice. A role the provider does not know also uses the language's voice.
- **The fingerprint holds the resolved voice, never the role name.** So two roles with the same voice share a fingerprint, and role `None` keeps the fingerprint (and every batch key) that existed before roles.
- `Capabilities::needs_approval` lists the document ratings (`rawr_source::Rating`) that need explicit approval before their text is sent. The provider sets the policy. `rawr-audiobook` enforces it. ElevenLabs and the fake both use `[Explicit, Unrated]`, so tests run the real gate. This is the only reason `rawr-tts` depends on `rawr-source` (no features).
- Providers classify errors. Only `RateLimited` and `Unavailable` are retryable. The caller owns retries and back-off.
- `AlignmentMismatch` carries the paid `Synthesis`. The caller must store it before it fails.

## Types

- Times are whole milliseconds (`CharacterTiming`).
- `AudioFormat::Pcm` is signed 16-bit little-endian mono with no header.
- `Receipt.headers` keeps every response header except credentials and cookies. Raw response bodies are not kept. They appear only in `trace`-level logs.

## Providers

- `fake` (feature `fake`): deterministic, no network, no cost. PCM at 48 kHz, 60 ms for each character, a quiet tone for letters and digits, and exact alignment. Its capabilities match ElevenLabs, so the whole pipeline can run against it in tests. `prepare` maps `[`→`(` and `]`→`)` like ElevenLabs. A role speaks with a voice named after the role, so a role changes the fingerprint and the voice name.

- `elevenlabs` (feature `elevenlabs`): `POST /v1/text-to-speech/{voice}/with-timestamps?output_format=…`, with a 120 s timeout.
  - `ElevenLabsSettings::new` has the defaults proven in purr: `eleven_v4`, Opus 48 kHz 128 kb/s, stability 0.35, similarity boost 0.75, concurrency 3. `voices` maps a language code to a voice ID. `roles` maps a voice role to a voice ID, and is empty by default. A role that is not in `roles` uses `voices[language]`.
  - `request_url`, `request_json` and `parse_response` are public and need no network. `role_request_url` and `role_voice_url` are the role-aware forms, and `synthesise` uses them with `request.role`. Tests use them, and so can tools that read archived responses.
  - `request_json` cuts continuity to the limits: the last 3 previous request IDs and the first 300 characters of the next text.
  - The `character-cost` header is whole credits. HTTP 401 with `quota_exceeded` is `QuotaExceeded`. Other 401 and 403 responses are `Unauthorised`.
  - `ApiKey` prints as `ApiKey(***)`. The receipt drops `authorization`, `cookie`, `set-cookie` and `xi-api-key` headers.
  - `reqwest` uses the same version and features as `rawr-storage`'s `s3` feature (0.12, `rustls-tls`), so a default `rawr` build compiles nothing new. Keep them the same.

## Known lint

`clippy::double_must_use` fires on the `#[async_trait]` expansion. `rawr-storage` has the same warning. Leave it until the trait moves to native async.

## Tests

Tests are in `tests/` (git-ignored, kept in `rawr-fixtures`). The ElevenLabs tests use real responses archived by purr, in the workspace's `tests/fixtures/elevenlabs/` (`exchange.json` with the request and headers, and `response.body`). They check that the rebuilt request URL and body equal what purr sent, and that the archived responses parse. There is no live (paid) test.

Run in the `rawr` toolbox:

```sh
cargo test -p rawr-tts --all-features
cargo test -p rawr-tts --features fake
cargo test -p rawr-tts --features elevenlabs
cargo test -p rawr-tts
cargo clippy -p rawr-tts --all-features --all-targets
cargo fmt -p rawr-tts --check
```
