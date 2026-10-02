# rawr-source

Content input for narration. A `Source` turns decompressed input bytes into a `Document`. The rest of the audiobook pipeline reads only `DocumentMeta` and `Section`.

## Model

- `Document`: `meta`, `sections`, `origin`.
- `Section` has the same shape as `rawr_extract::models::ChapterContent`. `body_xhtml`, `summary`, `notes` and `end_notes` are XHTML fragments, not whole documents.
- `DocumentMeta.summary` is plain text, not XHTML.
- `Origin` is `#[non_exhaustive]`. The pipeline must not match on it. Only the edges use it: the rating gate, path templates (`PathGenerator` needs a `Version`), and the AO3 title page.

## Sources

- `Ao3Html` (feature `ao3`) accepts AO3 HTML downloads only. It does not accept AO3 PDF or EPUB downloads, the live site, or rawr's cache database.
- `to_document` parses the input twice. `rawr_extract::extract` parses the header (about 25 KiB) for the `Version`. `Extractor::from_html` parses the full document for the chapters. The cost is small, and rawr-extract stays unchanged.
- `DocumentMeta.identifier` for AO3 is the work URL (`https://archiveofourown.org/works/{id}`). It is a valid EPUB `dc:identifier`.
- `DocumentMeta.language` is `Language::iso_code`. It is `None` when AO3 gives a language name that rawr-extract does not know.

## Pitfalls

- **Summary format and feature unification.** rawr-extract converts the work summary to Markdown when its `markdown` feature is on. This crate does not turn that feature on. But if another crate in the same build turns it on, Cargo unifies the features, and `DocumentMeta.summary` becomes Markdown.

## Tests

- Tests live in `tests/`, not in the source files. They use the AO3 fixtures in the workspace's `tests/fixtures/` (bzip2-compressed HTML downloads).
- `tests/` and the fixtures are git-ignored, because the fixtures are real fics. They are kept in the separate `rawr-fixtures` repository.
- Each fixture test is an `rstest` `#[files(...)]` case. The expected values come from `rawr-extract` itself, so the tests check the mapping, not the fic content.

## Checks

Run in the `rawr` toolbox:

```sh
cargo test -p rawr-source --features ao3
cargo test -p rawr-source
cargo clippy -p rawr-source --features ao3 --all-targets
cargo fmt -p rawr-source --check
```
