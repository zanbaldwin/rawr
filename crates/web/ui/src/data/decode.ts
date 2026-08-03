// Wire payload → in-memory Library. This is the one place the wire format
// is trusted-but-verified: `assertIndexPayload` checks the shape invariants
// (a CSR off-by-one silently drops works from results, and nobody will
// notice by eye — that's why this module is the most-tested in the app).

import type { LibraryIndex } from "../types/generated/LibraryIndex";
import type { Csr, Library, SeriesCsr } from "../types";

/** Bump whenever this module's output shape changes; cached copies with
 * another value are discarded and refetched. */
export const SCHEMA = 1;

/** The wire `format` this decoder understands. */
export const WIRE_FORMAT = 1;

export class DecodeError extends Error {}

function fail(message: string): never {
  throw new DecodeError(message);
}

/** Shape check for an untrusted payload: every column exactly `count`
 * long, every dictionary reference in range. */
export function assertIndexPayload(index: LibraryIndex): void {
  if (index.format !== WIRE_FORMAT) fail(`unsupported index format ${index.format}`);
  const w = index.works;
  const columns: [string, { length: number }][] = [
    ["id", w.id],
    ["hash", w.hash],
    ["cid", w.cid],
    ["title", w.title],
    ["authors", w.authors],
    ["fandoms", w.fandoms],
    ["series", w.series],
    ["tags", w.tags],
    ["language", w.language],
    ["rating", w.rating],
    ["warnings", w.warnings],
    ["words", w.words],
    ["chapters", w.chapters],
    ["chapters_total", w.chapters_total],
    ["complete", w.complete],
    ["summary", w.summary],
    ["published", w.published],
    ["updated", w.updated],
    ["added", w.added],
    ["versions", w.versions],
    ["bytes", w.bytes],
  ];
  for (const [name, column] of columns) {
    if (column.length !== index.count) fail(`column ${name}: ${column.length} entries, expected ${index.count}`);
  }
  if (index.dict.tag_kinds.length !== index.dict.tags.length) fail("tag_kinds/tags length mismatch");
  const checkRefs = (name: string, rows: number[][], limit: number) => {
    for (const row of rows) {
      for (const ref of row) if (ref >= limit) fail(`${name} reference ${ref} out of range (< ${limit})`);
    }
  };
  checkRefs("authors", w.authors, index.dict.authors.length);
  checkRefs("fandoms", w.fandoms, index.dict.fandoms.length);
  checkRefs("tags", w.tags, index.dict.tags.length);
  for (const row of w.series) {
    for (const ref of row) if (ref.i >= index.dict.series.length) fail(`series reference ${ref.i} out of range`);
  }
  for (const lang of w.language) {
    if (lang >= index.dict.languages.length) fail(`language reference ${lang} out of range`);
  }
}

function toCsr(rows: number[][]): Csr {
  const offsets = new Uint32Array(rows.length + 1);
  let total = 0;
  for (let i = 0; i < rows.length; i++) {
    total += rows[i]!.length;
    offsets[i + 1] = total;
  }
  const values = new Uint32Array(total);
  let cursor = 0;
  for (const row of rows) {
    for (const value of row) values[cursor++] = value;
  }
  return { offsets, values };
}

/** Argsort descending by `key`, stable via row-index tie-break (lower
 * row = more recently added, since wire order is recent-first). */
function permutationDesc(count: number, key: (i: number) => number): Uint32Array {
  const perm = new Uint32Array(count);
  for (let i = 0; i < count; i++) perm[i] = i;
  return perm.sort((a, b) => key(b) - key(a) || a - b);
}

export function decode(index: LibraryIndex): Library {
  assertIndexPayload(index);
  const w = index.works;
  const count = index.count;

  const seriesOffsets = new Uint32Array(count + 1);
  let seriesTotal = 0;
  for (let i = 0; i < count; i++) {
    seriesTotal += w.series[i]!.length;
    seriesOffsets[i + 1] = seriesTotal;
  }
  const series: SeriesCsr = {
    offsets: seriesOffsets,
    series: new Uint32Array(seriesTotal),
    positions: new Uint32Array(seriesTotal),
  };
  let cursor = 0;
  for (const row of w.series) {
    for (const ref of row) {
      series.series[cursor] = ref.i;
      series.positions[cursor] = ref.pos;
      cursor++;
    }
  }

  const words = Uint32Array.from(w.words);
  const updated = Int32Array.from(w.updated);

  const library: Library = {
    schema: SCHEMA,
    snapshot: index.snapshot,
    generatedAt: index.generated_at,
    target: index.target,
    count,
    enums: index.enums,
    dict: {
      authors: index.dict.authors,
      authorsSearch: index.dict.authors.map((a) => (a.p ? `${a.u} ${a.p}` : a.u).toLowerCase()),
      fandoms: index.dict.fandoms,
      languages: index.dict.languages,
      series: index.dict.series,
      tags: index.dict.tags,
      tagKinds: Uint8Array.from(index.dict.tag_kinds),
    },
    id: Uint32Array.from(w.id),
    hash: w.hash,
    cid: w.cid,
    title: w.title,
    titleLower: w.title.map((t) => t.toLowerCase()),
    authors: toCsr(w.authors),
    fandoms: toCsr(w.fandoms),
    tags: toCsr(w.tags),
    series,
    language: Uint16Array.from(w.language),
    rating: Uint8Array.from(w.rating),
    warnings: Uint8Array.from(w.warnings),
    words,
    chapters: Uint16Array.from(w.chapters, (c) => Math.min(c, 0xffff)),
    chaptersTotal: Int32Array.from(w.chapters_total),
    complete: Uint8Array.from(w.complete),
    summary: w.summary,
    published: Int32Array.from(w.published),
    updated,
    added: Float64Array.from(w.added),
    versions: Uint16Array.from(w.versions, (v) => Math.min(v, 0xffff)),
    bytes: Uint32Array.from(w.bytes),
    perm: {
      // Wire order is already most-recently-added first.
      recent: permutationDesc(count, (i) => -i),
      words: permutationDesc(count, (i) => words[i]!),
      updated: permutationDesc(count, (i) => updated[i]!),
    },
  };
  return library;
}

/** Built lazily on first title sort (the collator pass over 10k strings
 * costs tens of milliseconds) and then kept on the object — which also
 * persists it through the IndexedDB structured clone. */
export function titlePermutation(library: Library): Uint32Array {
  if (library.perm.title) return library.perm.title;
  const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });
  const perm = new Uint32Array(library.count);
  for (let i = 0; i < library.count; i++) perm[i] = i;
  perm.sort((a, b) => collator.compare(library.title[a]!, library.title[b]!) || a - b);
  library.perm.title = perm;
  return perm;
}
