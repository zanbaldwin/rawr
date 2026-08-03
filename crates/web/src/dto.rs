//! Wire-format DTOs for the JSON API.
//!
//! This mapping layer is mandatory, not optional: `rawr-extract`'s serde
//! impls are a *storage* encoding (`Author` serialises as a lossy
//! `"pseud (username)"` string; `Version`/`FileInfo` have none at all), so
//! nothing in here reuses them. Types derive `ts_rs::TS` behind the `ts`
//! feature; `cargo test -p rawr-web --features ts` writes the committed
//! TypeScript bindings the SPA compiles against.
//!
//! The index payload is columnar + dictionary-encoded: one row per work
//! (its *best* version, ranked by `Version::partial_cmp`), string tables
//! for everything that repeats, and dense numeric columns with sentinels
//! instead of nulls so the client can back them with typed arrays.
//! Summaries stay in the index (measured: ~2.8 MB of ~6.6 MB raw) because
//! the offline detail view renders from the index alone.

use crate::error::{ErrorKind, Result};
use exn::{OptionExt, ResultExt};
use rawr_config::models::FandomConfig;
use rawr_extract::models::{Author, Rating, SeriesPosition, TagKind, Version, Warning};
use rawr_storage::file::{FileInfo, Processed};
use serde::Serialize;
use std::collections::HashMap;
use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime, UtcDateTime};

type File = FileInfo<Processed>;

/// Bump when the wire shape changes incompatibly; the client refuses
/// payloads with an unexpected value and re-syncs.
pub const INDEX_FORMAT: u32 = 1;

#[cfg(feature = "ts")]
use ts_rs::TS;

/* ============ *\
|  Enum tables   |
\* ============ */

/// One row of a code table shipped in the index, so clients never
/// hard-code display labels.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct CodeLabel {
    pub code: u8,
    pub short: &'static str,
    pub label: &'static str,
}

/// The complete code tables for every enum the index encodes numerically.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct EnumTables {
    pub ratings: Vec<CodeLabel>,
    pub warnings: Vec<CodeLabel>,
    pub tag_kinds: Vec<CodeLabel>,
}

/// Dense rating code. `None` (rating never extracted) is distinct from
/// AO3's explicit "Not Rated".
///
/// Exhaustive on purpose: a new upstream variant becomes a compile error
/// here, not a missing label in the client.
pub fn rating_code(rating: Option<Rating>) -> u8 {
    match rating {
        Some(Rating::GeneralAudiences) => 0,
        Some(Rating::TeenAndUp) => 1,
        Some(Rating::Mature) => 2,
        Some(Rating::Explicit) => 3,
        Some(Rating::NotRated) => 4,
        None => 5,
    }
}

/// Bit value for one archive warning. Exhaustive on purpose (see
/// [`rating_code`]); six variants fit comfortably in the `u8` bitmask.
pub const fn warning_bit(warning: Warning) -> u8 {
    match warning {
        Warning::NoWarningsApply => 1,
        Warning::CreatorChoseNotToWarn => 2,
        Warning::GraphicViolence => 4,
        Warning::MajorCharacterDeath => 8,
        Warning::Underage => 16,
        Warning::NonCon => 32,
    }
}

/// Dense tag-kind code. Exhaustive on purpose (see [`rating_code`]).
pub fn tag_kind_code(kind: TagKind) -> u8 {
    match kind {
        TagKind::Relationship => 0,
        TagKind::Character => 1,
        TagKind::Freeform => 2,
    }
}

impl EnumTables {
    pub fn current() -> Self {
        let rating = |r: Option<Rating>, short: &'static str, label: &'static str| CodeLabel {
            code: rating_code(r),
            short,
            label,
        };
        let warning = |w: Warning| CodeLabel {
            code: warning_bit(w),
            short: warning_short(w),
            label: w.as_str(),
        };
        let kind = |k: TagKind, short: &'static str| CodeLabel {
            code: tag_kind_code(k),
            short,
            label: k.as_str(),
        };
        Self {
            ratings: vec![
                rating(
                    Some(Rating::GeneralAudiences),
                    Rating::GeneralAudiences.as_short_str(),
                    Rating::GeneralAudiences.as_str(),
                ),
                rating(Some(Rating::TeenAndUp), Rating::TeenAndUp.as_short_str(), Rating::TeenAndUp.as_str()),
                rating(Some(Rating::Mature), Rating::Mature.as_short_str(), Rating::Mature.as_str()),
                rating(Some(Rating::Explicit), Rating::Explicit.as_short_str(), Rating::Explicit.as_str()),
                rating(Some(Rating::NotRated), Rating::NotRated.as_short_str(), Rating::NotRated.as_str()),
                rating(None, "?", "Unknown"),
            ],
            warnings: vec![
                warning(Warning::NoWarningsApply),
                warning(Warning::CreatorChoseNotToWarn),
                warning(Warning::GraphicViolence),
                warning(Warning::MajorCharacterDeath),
                warning(Warning::Underage),
                warning(Warning::NonCon),
            ],
            tag_kinds: vec![
                kind(TagKind::Relationship, "relationship"),
                kind(TagKind::Character, "character"),
                kind(TagKind::Freeform, "freeform"),
            ],
        }
    }
}

const fn warning_short(warning: Warning) -> &'static str {
    match warning {
        Warning::NoWarningsApply => "none",
        Warning::CreatorChoseNotToWarn => "chose-not-to-warn",
        Warning::GraphicViolence => "violence",
        Warning::MajorCharacterDeath => "death",
        Warning::Underage => "underage",
        Warning::NonCon => "noncon",
    }
}

/* ============== *\
|  Library index   |
\* ============== */

/// An author dictionary entry. Deliberately an object, never the storage
/// encoding's `"pseud (username)"` string — clients must not re-implement
/// that parser.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AuthorEntry {
    /// Username.
    pub u: String,
    /// Pseudonym, when it differs from the username.
    pub p: Option<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct LanguageEntry {
    pub name: String,
    pub iso: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct SeriesEntry {
    pub id: u32,
    pub name: String,
}

/// A work's membership in a series: dictionary index + position.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct SeriesRef {
    /// Index into `dict.series`.
    pub i: u32,
    pub pos: u32,
}

/// String tables referenced by index columns via `u32` positions.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct Dictionaries {
    pub authors: Vec<AuthorEntry>,
    pub fandoms: Vec<String>,
    pub languages: Vec<LanguageEntry>,
    pub series: Vec<SeriesEntry>,
    pub tags: Vec<String>,
    /// Tag-kind code for each entry of `tags`, parallel array.
    pub tag_kinds: Vec<u8>,
}

/// Columnar per-work data; every column has exactly `count` entries.
///
/// Numeric columns use dense sentinels rather than nulls so the client can
/// back them with typed arrays: `chapters_total == -1` means "?", rating
/// code `5` means unknown. `summary` is the only nullable column.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct WorkColumns {
    /// AO3 work id.
    pub id: Vec<u32>,
    /// crc32 of the best version, `{:08x}` — rawr's short display id.
    pub hash: Vec<String>,
    /// Full 64-hex content hash of the best version; addresses the
    /// immutable EPUB URL.
    pub cid: Vec<String>,
    pub title: Vec<String>,
    /// Indices into `dict.authors`.
    pub authors: Vec<Vec<u32>>,
    /// Indices into `dict.fandoms`.
    pub fandoms: Vec<Vec<u32>>,
    pub series: Vec<Vec<SeriesRef>>,
    /// Indices into `dict.tags`.
    pub tags: Vec<Vec<u32>>,
    /// Index into `dict.languages`.
    pub language: Vec<u32>,
    /// See `enums.ratings`.
    pub rating: Vec<u8>,
    /// Bitmask; see `enums.warnings`.
    pub warnings: Vec<u8>,
    pub words: Vec<u32>,
    pub chapters: Vec<u32>,
    /// `-1` when the total is unknown ("?").
    pub chapters_total: Vec<i32>,
    /// `1`/`0`; kept numeric for typed-array parity.
    pub complete: Vec<u8>,
    /// Raw Markdown, exactly as stored. Do not render as HTML unescaped.
    pub summary: Vec<Option<String>>,
    /// Days since the Unix epoch (UTC midnight).
    pub published: Vec<i32>,
    /// Days since the Unix epoch (UTC midnight).
    pub updated: Vec<i32>,
    /// Unix seconds the work most recently landed in the library.
    #[cfg_attr(feature = "ts", ts(type = "number[]"))]
    pub added: Vec<i64>,
    /// How many versions of this work the library holds.
    pub versions: Vec<u32>,
    /// Decompressed size of the best version, in bytes.
    pub bytes: Vec<u32>,
}

/// The whole-library sync payload.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct LibraryIndex {
    pub format: u32,
    /// Change token; equals the endpoint's ETag payload. Filled in by the
    /// endpoint after serialisation — [`LibraryIndex::build`] leaves it
    /// empty.
    pub snapshot: String,
    /// Unix seconds.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub generated_at: i64,
    pub target: String,
    pub count: u32,
    pub enums: EnumTables,
    pub dict: Dictionaries,
    pub works: WorkColumns,
}

/// Interns values in first-seen order; deterministic because row order is.
struct Interner<K> {
    lookup: HashMap<K, u32>,
    entries: Vec<K>,
}

impl<K: Clone + Eq + std::hash::Hash> Interner<K> {
    fn new() -> Self {
        Self {
            lookup: HashMap::new(),
            entries: Vec::new(),
        }
    }

    fn intern(&mut self, key: &K) -> u32 {
        if let Some(&idx) = self.lookup.get(key) {
            return idx;
        }
        let idx = u32::try_from(self.entries.len()).expect("dictionary exceeds u32 entries");
        self.lookup.insert(key.clone(), idx);
        self.entries.push(key.clone());
        idx
    }
}

impl LibraryIndex {
    /// Build the index from [`Repository::list_works_for_target`] output:
    /// works most-recently-added first, versions best-first within each.
    ///
    /// Fandom names pass through [`FandomConfig::display_name`] while
    /// interning, so configured renames collapse alias variants into one
    /// dictionary entry — the browse filter shows canonical names with
    /// merged counts, mirroring the CLI's stats behaviour.
    ///
    /// [`Repository::list_works_for_target`]: rawr_cache::Repository::list_works_for_target
    pub fn build(
        target: &str,
        generated_at: UtcDateTime,
        works: &[(u64, Vec<(Version, Vec<File>)>)],
        fandom_config: &FandomConfig,
    ) -> Result<Self> {
        let mut authors: Interner<AuthorEntry> = Interner::new();
        let mut fandoms: Interner<String> = Interner::new();
        let mut languages: Interner<LanguageEntry> = Interner::new();
        let mut series: Interner<(u32, String)> = Interner::new();
        let mut tags: Interner<(String, u8)> = Interner::new();

        let count = u32::try_from(works.len()).or_raise(|| ErrorKind::Index)?;
        let mut columns = WorkColumns {
            id: Vec::with_capacity(works.len()),
            hash: Vec::with_capacity(works.len()),
            cid: Vec::with_capacity(works.len()),
            title: Vec::with_capacity(works.len()),
            authors: Vec::with_capacity(works.len()),
            fandoms: Vec::with_capacity(works.len()),
            series: Vec::with_capacity(works.len()),
            tags: Vec::with_capacity(works.len()),
            language: Vec::with_capacity(works.len()),
            rating: Vec::with_capacity(works.len()),
            warnings: Vec::with_capacity(works.len()),
            words: Vec::with_capacity(works.len()),
            chapters: Vec::with_capacity(works.len()),
            chapters_total: Vec::with_capacity(works.len()),
            complete: Vec::with_capacity(works.len()),
            summary: Vec::with_capacity(works.len()),
            published: Vec::with_capacity(works.len()),
            updated: Vec::with_capacity(works.len()),
            added: Vec::with_capacity(works.len()),
            versions: Vec::with_capacity(works.len()),
            bytes: Vec::with_capacity(works.len()),
        };

        for (work_id, versions) in works {
            let (best, _) = versions.first().ok_or_raise(|| ErrorKind::Index)?;
            let m = &best.metadata;
            // A silent wrap here would corrupt every URL for the work.
            columns.id.push(u32::try_from(*work_id).or_raise(|| ErrorKind::Index)?);
            columns.hash.push(format!("{:08x}", best.crc32));
            columns.cid.push(best.hash.clone());
            columns.title.push(m.title.clone());
            columns.authors.push(m.authors.iter().map(|a| authors.intern(&author_entry(a))).collect());
            // Canonicalise, then dedupe: a work tagged with two aliases of
            // the same fandom must not reference the entry twice (facet
            // counts would double).
            let mut fandom_refs: Vec<u32> = Vec::with_capacity(m.fandoms.len());
            for fandom in &m.fandoms {
                let index = fandoms.intern(&fandom_config.display_name(&fandom.name).to_string());
                if !fandom_refs.contains(&index) {
                    fandom_refs.push(index);
                }
            }
            columns.fandoms.push(fandom_refs);
            columns.series.push(m.series.iter().map(|s| series_ref(s, &mut series)).collect::<Result<Vec<_>>>()?);
            columns.tags.push(m.tags.iter().map(|t| tags.intern(&(t.name.clone(), tag_kind_code(t.kind)))).collect());
            columns.language.push(languages.intern(&LanguageEntry {
                name: m.language.name.clone(),
                iso: m.language.iso_code,
            }));
            columns.rating.push(rating_code(m.rating));
            columns.warnings.push(m.warnings.iter().fold(0u8, |acc, w| acc | warning_bit(*w)));
            columns.words.push(u32::try_from(m.words).or_raise(|| ErrorKind::Index)?);
            columns.chapters.push(m.chapters.written);
            columns.chapters_total.push(m.chapters.total.map_or(-1, |t| i32::try_from(t).unwrap_or(i32::MAX)));
            columns.complete.push(u8::from(m.chapters.is_complete()));
            columns.summary.push(m.summary.clone());
            columns.published.push(date_to_days(m.published));
            columns.updated.push(date_to_days(m.last_modified));
            columns.added.push(
                versions
                    .iter()
                    .flat_map(|(_, files)| files.iter().map(|f| f.discovered_at.unix_timestamp()))
                    .max()
                    .unwrap_or(0),
            );
            columns.versions.push(u32::try_from(versions.len()).or_raise(|| ErrorKind::Index)?);
            columns.bytes.push(u32::try_from(best.length).or_raise(|| ErrorKind::Index)?);
        }

        let (tag_names, tag_kinds) = tags.entries.into_iter().unzip();
        Ok(Self {
            format: INDEX_FORMAT,
            snapshot: String::new(),
            generated_at: generated_at.unix_timestamp(),
            target: target.to_string(),
            count,
            enums: EnumTables::current(),
            dict: Dictionaries {
                authors: authors.entries,
                fandoms: fandoms.entries,
                languages: languages.entries,
                series: series.entries.into_iter().map(|(id, name)| SeriesEntry { id, name }).collect(),
                tags: tag_names,
                tag_kinds,
            },
            works: columns,
        })
    }
}

fn author_entry(author: &Author) -> AuthorEntry {
    AuthorEntry {
        u: author.username.clone(),
        p: author.pseudonym.clone(),
    }
}

fn series_ref(position: &SeriesPosition, interner: &mut Interner<(u32, String)>) -> Result<SeriesRef> {
    let id = u32::try_from(position.id).or_raise(|| ErrorKind::Index)?;
    Ok(SeriesRef {
        i: interner.intern(&(id, position.name.clone())),
        pos: position.position,
    })
}

/* =========== *\
|  Detail DTOs  |
\* =========== */

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AuthorDto {
    pub username: String,
    pub pseudonym: Option<String>,
    /// Ready-to-render byline segment.
    pub display: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct RatingDto {
    pub code: u8,
    pub short: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct WarningDto {
    pub code: u8,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct TagDto {
    pub name: String,
    pub kind: u8,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct SeriesDto {
    pub id: u32,
    pub name: String,
    pub position: u32,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ChaptersDto {
    pub written: u32,
    pub total: Option<u32>,
    pub complete: bool,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct LanguageDto {
    pub name: String,
    pub iso: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct MetadataDto {
    pub title: String,
    pub authors: Vec<AuthorDto>,
    pub fandoms: Vec<String>,
    pub series: Vec<SeriesDto>,
    pub rating: RatingDto,
    pub warnings: Vec<WarningDto>,
    pub tags: Vec<TagDto>,
    /// Raw Markdown, exactly as stored.
    pub summary: Option<String>,
    pub language: LanguageDto,
    pub chapters: ChaptersDto,
    pub words: u32,
    /// ISO date, `YYYY-MM-DD`.
    pub published: String,
    /// ISO date, `YYYY-MM-DD`.
    pub last_modified: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct FileDto {
    pub target: String,
    pub path: String,
    pub compression: String,
    pub size: u32,
    pub file_hash: String,
    /// RFC 3339, whole seconds.
    pub discovered_at: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct VersionDto {
    /// Full 64-hex content hash.
    pub content_hash: String,
    /// crc32 short display id, `{:08x}`.
    pub id: String,
    /// Decompressed size in bytes.
    pub length: u32,
    /// RFC 3339, whole seconds.
    pub extracted_at: String,
    /// True `Version::partial_cmp` ranking put this version first.
    pub is_best: bool,
    pub metadata: MetadataDto,
    pub files: Vec<FileDto>,
    /// Immutable download URL for this exact version.
    pub epub_url: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct WorkDetail {
    pub work_id: u32,
    /// The work's page on AO3.
    pub url: String,
    /// Versions best-first; `versions[0]` is the one to present.
    pub versions: Vec<VersionDto>,
}

impl WorkDetail {
    /// Build from [`Repository::get_by_work_id`] output (versions already
    /// best-first).
    ///
    /// [`Repository::get_by_work_id`]: rawr_cache::Repository::get_by_work_id
    pub fn build(work_id: u64, versions: &[(Version, Vec<File>)]) -> Result<Self> {
        let work_id = u32::try_from(work_id).or_raise(|| ErrorKind::Index)?;
        let versions = versions
            .iter()
            .enumerate()
            .map(|(rank, (version, files))| version_dto(version, files, rank == 0))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            work_id,
            url: format!("https://archiveofourown.org/works/{work_id}"),
            versions,
        })
    }
}

fn version_dto(version: &Version, files: &[File], is_best: bool) -> Result<VersionDto> {
    let m = &version.metadata;
    let rating_table = EnumTables::current().ratings;
    let rating = rating_table
        .into_iter()
        .find(|row| row.code == rating_code(m.rating))
        .map(|row| RatingDto {
            code: row.code,
            short: row.short.to_string(),
            label: row.label.to_string(),
        })
        .ok_or_raise(|| ErrorKind::Index)?;
    Ok(VersionDto {
        content_hash: version.hash.clone(),
        id: format!("{:08x}", version.crc32),
        length: u32::try_from(version.length).or_raise(|| ErrorKind::Index)?,
        extracted_at: datetime_rfc3339(version.extracted_at)?,
        is_best,
        metadata: MetadataDto {
            title: m.title.clone(),
            authors: m
                .authors
                .iter()
                .map(|a| AuthorDto {
                    username: a.username.clone(),
                    pseudonym: a.pseudonym.clone(),
                    display: a.to_string(),
                })
                .collect(),
            fandoms: m.fandoms.iter().map(|f| f.name.clone()).collect(),
            series: m
                .series
                .iter()
                .map(|s| {
                    Ok(SeriesDto {
                        id: u32::try_from(s.id).or_raise(|| ErrorKind::Index)?,
                        name: s.name.clone(),
                        position: s.position,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            rating,
            warnings: m
                .warnings
                .iter()
                .map(|w| WarningDto {
                    code: warning_bit(*w),
                    label: w.as_str().to_string(),
                })
                .collect(),
            tags: m
                .tags
                .iter()
                .map(|t| TagDto {
                    name: t.name.clone(),
                    kind: tag_kind_code(t.kind),
                })
                .collect(),
            summary: m.summary.clone(),
            language: LanguageDto {
                name: m.language.name.clone(),
                iso: m.language.iso_code,
            },
            chapters: ChaptersDto {
                written: m.chapters.written,
                total: m.chapters.total,
                complete: m.chapters.is_complete(),
            },
            words: u32::try_from(m.words).or_raise(|| ErrorKind::Index)?,
            published: date_iso(m.published),
            last_modified: date_iso(m.last_modified),
        },
        files: files
            .iter()
            .map(|f| {
                Ok(FileDto {
                    target: f.target.clone(),
                    path: f.path.to_string(),
                    compression: f.compression.to_string(),
                    size: u32::try_from(f.size).or_raise(|| ErrorKind::Index)?,
                    file_hash: f.file_hash.clone(),
                    discovered_at: datetime_rfc3339(f.discovered_at)?,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        epub_url: format!("/api/v1/versions/{}/download.epub", version.hash),
    })
}

/* ================= *\
|  Meta/stats/upload  |
\* ================= */

/// The JSON error body every `/api/v1` error response carries — the
/// contract is "every API response is JSON, always".
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ApiError {
    /// Stable machine slug (`not_found`, `too_large`, …) from an
    /// exhaustive match — never `Display` output.
    pub kind: String,
    pub status: u16,
    pub message: String,
    /// The underlying error chain, outermost first. This is a no-auth LAN
    /// tool for its own author: reading the real cause on the iPad beats
    /// hiding it.
    pub chain: Vec<String>,
}

/// Tiny "is the NAS awake, has anything changed" probe.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ApiMeta {
    pub name: &'static str,
    pub version: &'static str,
    pub target: String,
    pub dry_run: bool,
    /// Current index change token (the index endpoint's ETag payload).
    pub snapshot: String,
    pub works: u32,
    pub versions: u32,
    /// Unix seconds.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub generated_at: i64,
}

/// Storage sizes — the only stats the client cannot derive from the index.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct StorageStatsDto {
    /// Decompressed bytes across best versions.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub content_size: u64,
    /// On-disk (compressed) bytes.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub file_size: u64,
}

/// A successfully imported (or matched) file's identity.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct UploadedWork {
    pub work_id: u32,
    pub title: String,
    /// crc32 short display id, `{:08x}`.
    pub hash: String,
    pub path: String,
    pub size: u32,
    pub compression: String,
}

/// Per-file import outcome; maps 1:1 onto `rawr_library`'s `Import` enum
/// plus a `failed` arm. Deliberately not the CLI's `Reason`, which is
/// coupled to console styling.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub enum UploadOutcome {
    Added { work: UploadedWork },
    Upgraded { work: UploadedWork },
    Unchanged { work: UploadedWork },
    Outdated { work: UploadedWork },
    Failed { message: String, kind: String },
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct UploadResult {
    pub filename: String,
    pub outcome: UploadOutcome,
}

#[derive(Clone, Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct UploadSummary {
    pub added: u32,
    pub upgraded: u32,
    pub unchanged: u32,
    pub outdated: u32,
    pub failed: u32,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct UploadResponse {
    pub dry_run: bool,
    /// Post-import change token — differs from the client's stored one
    /// when the upload changed the library.
    pub snapshot: String,
    pub results: Vec<UploadResult>,
    pub summary: UploadSummary,
}

/* ========= *\
|  Encoding   |
\* ========= */

/// Days since the Unix epoch for a UTC-midnight date. Exact: the cache
/// stores dates as midnight-UTC unix seconds.
pub fn date_to_days(date: Date) -> i32 {
    let seconds = date.midnight().assume_utc().unix_timestamp();
    i32::try_from(seconds.div_euclid(86_400)).expect("date within i32 day range")
}

/// `YYYY-MM-DD`.
pub fn date_iso(date: Date) -> String {
    format!("{:04}-{:02}-{:02}", date.year(), u8::from(date.month()), date.day())
}

/// RFC 3339 with whole seconds (nanoseconds are stripped on DB round-trip
/// anyway).
pub fn datetime_rfc3339(datetime: UtcDateTime) -> Result<String> {
    OffsetDateTime::from_unix_timestamp(datetime.unix_timestamp())
        .or_raise(|| ErrorKind::Index)?
        .format(&Rfc3339)
        .or_raise(|| ErrorKind::Index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawr_extract::models::{ChapterCount, Fandom, Language, Metadata, Tag};
    use time::Month;

    fn version(work_id: u64, hash: &str, title: &str) -> Version {
        Version {
            hash: hash.to_string(),
            length: 1000,
            crc32: 0x37bc_3355,
            metadata: Metadata {
                work_id,
                title: title.to_string(),
                authors: vec![Author {
                    username: "user".into(),
                    pseudonym: Some("pseud".into()),
                }],
                fandoms: vec![Fandom { name: "Testing".into() }],
                rating: Some(Rating::Explicit),
                warnings: vec![Warning::NoWarningsApply, Warning::MajorCharacterDeath],
                tags: vec![Tag {
                    name: "Fluff".into(),
                    kind: TagKind::Freeform,
                }],
                summary: Some("A *test* work".into()),
                language: Language {
                    name: "English".into(),
                    iso_code: Some("en"),
                },
                chapters: ChapterCount { written: 3, total: None },
                words: 1234,
                published: Date::from_calendar_date(2020, Month::February, 29).unwrap(),
                last_modified: Date::from_calendar_date(2024, Month::June, 1).unwrap(),
                series: vec![SeriesPosition {
                    id: 77,
                    name: "Series".into(),
                    position: 2,
                }],
            },
            extracted_at: UtcDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        }
    }

    #[test]
    fn enum_tables_have_unique_codes() {
        let tables = EnumTables::current();
        let unique = |rows: &[CodeLabel]| {
            let codes: std::collections::HashSet<u8> = rows.iter().map(|r| r.code).collect();
            codes.len() == rows.len()
        };
        assert!(unique(&tables.ratings));
        assert!(unique(&tables.warnings));
        assert!(unique(&tables.tag_kinds));
        // Warning bits must pack into the u8 bitmask column.
        assert!(tables.warnings.len() <= 8);
        assert!(tables.warnings.iter().all(|w| w.code.is_power_of_two()));
    }

    #[test]
    fn date_encoding() {
        let date = Date::from_calendar_date(1970, Month::January, 1).unwrap();
        assert_eq!(date_to_days(date), 0);
        let date = Date::from_calendar_date(1969, Month::December, 31).unwrap();
        assert_eq!(date_to_days(date), -1);
        let date = Date::from_calendar_date(2020, Month::February, 29).unwrap();
        assert_eq!(date_iso(date), "2020-02-29");
        assert_eq!(date_to_days(date) as i64 * 86_400, date.midnight().assume_utc().unix_timestamp());
    }

    #[test]
    fn index_build_encodes_and_interns() {
        let v = version(12345, "a".repeat(64).as_str(), "Work A");
        let works = vec![(12345u64, vec![(v, vec![])])];
        let index = LibraryIndex::build(
            "local",
            UtcDateTime::from_unix_timestamp(1_800_000_000).unwrap(),
            &works,
            &FandomConfig::default(),
        )
        .expect("index builds");
        assert_eq!(index.format, INDEX_FORMAT);
        assert_eq!(index.count, 1);
        assert_eq!(index.works.id, vec![12345]);
        assert_eq!(index.works.hash, vec!["37bc3355"]);
        assert_eq!(index.works.warnings, vec![1 | 8]);
        assert_eq!(index.works.rating, vec![3]);
        assert_eq!(index.works.chapters_total, vec![-1]);
        assert_eq!(index.works.complete, vec![0]);
        assert_eq!(index.dict.fandoms, vec!["Testing"]);
        assert_eq!(index.dict.tags, vec!["Fluff"]);
        assert_eq!(index.dict.tag_kinds, vec![2]);
        assert_eq!(
            index.dict.authors,
            vec![AuthorEntry {
                u: "user".into(),
                p: Some("pseud".into())
            }]
        );
        assert_eq!(index.works.added, vec![0]);
        // Serialises without error and round-trips the sentinel.
        let json = serde_json::to_string(&index).unwrap();
        assert!(json.contains("\"chapters_total\":[-1]"));
    }

    #[test]
    fn index_build_applies_fandom_renames() {
        let mut a = version(100, &"a".repeat(64), "Work A");
        a.metadata.fandoms = vec![
            Fandom {
                name: "Spider-Man - All Media Types".into(),
            },
            // Two aliases of the same canonical fandom on one work…
            Fandom {
                name: "Spider-Man (Marvel) - Fandom".into(),
            },
        ];
        let mut b = version(200, &"b".repeat(64), "Work B");
        b.metadata.fandoms = vec![Fandom { name: "Unrelated Fandom".into() }];
        let works = vec![(100u64, vec![(a, vec![])]), (200u64, vec![(b, vec![])])];

        let mut config = FandomConfig::default();
        config.renames.insert(
            "Spider-Man".to_string(),
            vec![
                "Spider-Man - All Media Types".to_string(),
                "Spider-Man (Marvel) - Fandom".to_string(),
            ],
        );

        let index =
            LibraryIndex::build("local", UtcDateTime::from_unix_timestamp(1_800_000_000).unwrap(), &works, &config)
                .expect("index builds");
        // Aliases collapse into one canonical dictionary entry…
        assert_eq!(index.dict.fandoms, vec!["Spider-Man", "Unrelated Fandom"]);
        // …and the double-tagged work references it exactly once.
        assert_eq!(index.works.fandoms, vec![vec![0], vec![1]]);
    }

    #[test]
    fn work_detail_maps_fields() {
        let v = version(12345, &"b".repeat(64), "Work B");
        let detail = WorkDetail::build(12345, &[(v, vec![])]).expect("detail builds");
        assert_eq!(detail.work_id, 12345);
        assert_eq!(detail.url, "https://archiveofourown.org/works/12345");
        let best = &detail.versions[0];
        assert!(best.is_best);
        assert_eq!(best.id, "37bc3355");
        assert_eq!(best.metadata.rating.short, "E");
        assert_eq!(best.metadata.published, "2020-02-29");
        assert_eq!(best.extracted_at, "2023-11-14T22:13:20Z");
        assert_eq!(best.epub_url, format!("/api/v1/versions/{}/download.epub", "b".repeat(64)));
    }
}
