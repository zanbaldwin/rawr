// In-memory filter/sort over the decoded library. Target: ~1–3 ms for
// 10k works — no worker, no debounce needed.

import type { Csr, Library, Query } from "../types";
import { titlePermutation } from "./decode";

/** Does row `i` of `csr` contain any of `values`? Rows are tiny (1–3
 * entries) and selections small, so the nested loop beats a Set. */
function csrContainsAny(csr: Csr, i: number, values: readonly number[]): boolean {
  const end = csr.offsets[i + 1]!;
  for (let cursor = csr.offsets[i]!; cursor < end; cursor++) {
    for (const value of values) {
      if (csr.values[cursor] === value) return true;
    }
  }
  return false;
}

/** Mark every author whose name matches the needle — one pass over the
 * ~3k-entry dictionary instead of string work per work. */
function authorMask(library: Library, needle: string): Uint8Array {
  const mask = new Uint8Array(library.dict.authors.length);
  for (let i = 0; i < mask.length; i++) {
    if (library.dict.authorsSearch[i]!.includes(needle)) mask[i] = 1;
  }
  return mask;
}

function permutation(library: Library, sort: Query["sort"]): Uint32Array {
  switch (sort) {
    case "recent":
      return library.perm.recent;
    case "words":
      return library.perm.words;
    case "updated":
      return library.perm.updated;
    case "title":
      return titlePermutation(library);
  }
}

// Shared scratch buffer: grown to the library size once, reused across
// keystrokes. The result must always be `.slice()` (a copy) — a
// `.subarray()` view would be silently corrupted by the next query.
let scratch = new Uint32Array(0);

/** Run a query; returns matching row indices already in `query.sort`
 * order (the walk follows a precomputed permutation, so there is no
 * comparator work at query time). */
export function runQuery(library: Library, query: Query): Uint32Array {
  if (scratch.length < library.count) scratch = new Uint32Array(library.count);
  const needle = query.q;
  const authors = needle !== "" ? authorMask(library, needle) : null;
  const perm = permutation(library, query.sort);
  let n = 0;
  for (let p = 0; p < perm.length; p++) {
    const i = perm[p]!;
    // Cheapest predicates first.
    if (query.rating !== -1 && library.rating[i] !== query.rating) continue;
    if (query.complete !== -1 && library.complete[i] !== query.complete) continue;
    if (query.fandoms.length > 0 && !csrContainsAny(library.fandoms, i, query.fandoms)) continue;
    if (needle !== "" && !matchesText(library, i, needle, authors!)) continue;
    scratch[n++] = i;
  }
  return scratch.slice(0, n);
}

function matchesText(library: Library, i: number, needle: string, authors: Uint8Array): boolean {
  if (library.titleLower[i]!.includes(needle)) return true;
  const end = library.authors.offsets[i + 1]!;
  for (let cursor = library.authors.offsets[i]!; cursor < end; cursor++) {
    if (authors[library.authors.values[cursor]!] === 1) return true;
  }
  return false;
}

/** Facet counts for the fandom dropdown, computed with the fandom filter
 * itself excluded (each option shows what choosing it *would* give). */
export function fandomCounts(library: Library, query: Query): Uint32Array {
  const counts = new Uint32Array(library.dict.fandoms.length);
  const needle = query.q;
  const authors = needle !== "" ? authorMask(library, needle) : null;
  for (let i = 0; i < library.count; i++) {
    if (query.rating !== -1 && library.rating[i] !== query.rating) continue;
    if (query.complete !== -1 && library.complete[i] !== query.complete) continue;
    if (needle !== "" && !matchesText(library, i, needle, authors!)) continue;
    const end = library.fandoms.offsets[i + 1]!;
    for (let cursor = library.fandoms.offsets[i]!; cursor < end; cursor++) {
      const fandom = library.fandoms.values[cursor]!;
      counts[fandom] = counts[fandom]! + 1;
    }
  }
  return counts;
}
