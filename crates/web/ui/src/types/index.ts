// Hand-written in-memory types. `types/generated/` is the wire format and
// Rust owns it; these are the decoded, typed-array-backed shapes that have
// no Rust counterpart.

import type { AuthorEntry } from "./generated/AuthorEntry";
import type { EnumTables } from "./generated/EnumTables";
import type { LanguageEntry } from "./generated/LanguageEntry";
import type { SeriesEntry } from "./generated/SeriesEntry";

/** Compressed sparse rows: row i's values are `values[offsets[i]..offsets[i+1]]`. */
export interface Csr {
  offsets: Uint32Array;
  values: Uint32Array;
}

/** Series memberships in CSR form with a parallel position array. */
export interface SeriesCsr {
  offsets: Uint32Array;
  series: Uint32Array;
  positions: Uint32Array;
}

export type SortKey = "recent" | "title" | "words" | "updated";

/** The decoded library: struct-of-arrays over `count` works. */
export interface Library {
  /** Bumped whenever decode's output shape changes; a mismatch in the
   * IndexedDB cache means "refetch", which is the whole migration
   * strategy. */
  schema: number;
  snapshot: string;
  generatedAt: number;
  target: string;
  count: number;
  enums: EnumTables;
  dict: {
    authors: AuthorEntry[];
    /** Lowercased "username pseudonym" per author, for search. */
    authorsSearch: string[];
    fandoms: string[];
    languages: LanguageEntry[];
    series: SeriesEntry[];
    tags: string[];
    tagKinds: Uint8Array;
  };
  id: Uint32Array;
  /** crc32 short display id, 8-hex. */
  hash: string[];
  /** Full content hash, addresses the immutable EPUB URL. */
  cid: string[];
  title: string[];
  titleLower: string[];
  authors: Csr;
  fandoms: Csr;
  tags: Csr;
  series: SeriesCsr;
  language: Uint16Array;
  rating: Uint8Array;
  /** Bitmask; see `enums.warnings`. */
  warnings: Uint8Array;
  words: Uint32Array;
  chapters: Uint16Array;
  /** -1 = "?" */
  chaptersTotal: Int32Array;
  complete: Uint8Array;
  /** Raw Markdown or null. */
  summary: (string | null)[];
  /** Days since epoch. */
  published: Int32Array;
  /** Days since epoch. */
  updated: Int32Array;
  /** Unix seconds (fits f64 exactly). */
  added: Float64Array;
  versions: Uint16Array;
  bytes: Uint32Array;
  /** Precomputed sort permutations; `title` is built lazily (collator
   * cost) and persisted once built. */
  perm: {
    recent: Uint32Array;
    words: Uint32Array;
    updated: Uint32Array;
    title?: Uint32Array;
  };
}

/** A normalised list query. `-1` means "any". */
export interface Query {
  /** Lowercased needle; empty = no text filter. */
  q: string;
  /** Indices into `dict.fandoms`; empty = any. A work matches when it
   * belongs to ANY selected fandom (OR semantics). */
  fandoms: number[];
  /** Index into `dict.languages`. */
  language: number;
  /** 1 = complete only, 0 = incomplete only. */
  complete: number;
  sort: SortKey;
}

export const DEFAULT_QUERY: Query = { q: "", fandoms: [], language: -1, complete: -1, sort: "recent" };
