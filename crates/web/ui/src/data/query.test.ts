import { describe, expect, it } from "vitest";
import { DEFAULT_QUERY } from "../types";
import type { Query } from "../types";
import { decode } from "./decode";
import { syntheticIndex } from "./fixture";
import { fandomCounts, runQuery } from "./query";

const lib = decode(syntheticIndex());
const query = (overrides: Partial<Query>): Query => ({ ...DEFAULT_QUERY, ...overrides });

describe("runQuery", () => {
  it("returns everything, recent-first, with no filters", () => {
    expect(Array.from(runQuery(lib, query({})))).toEqual([0, 1, 2, 3]);
  });

  it("filters by title text", () => {
    expect(Array.from(runQuery(lib, query({ q: "charlie" })))).toEqual([2]);
    // "alpha" matches the title of row 1 AND the author of rows 0 and 2.
    expect(Array.from(runQuery(lib, query({ q: "alpha" })))).toEqual([0, 1, 2]);
  });

  it("filters by author username and pseudonym", () => {
    // "alpha" the author matches rows 0 and 2; "alpha work" title matches row 1.
    expect(Array.from(runQuery(lib, query({ q: "alph" })))).toEqual([0, 1, 2]);
    // Pseudonym "bee" matches rows with author beta (1 and 2).
    expect(Array.from(runQuery(lib, query({ q: "bee" })))).toEqual([1, 2]);
  });

  it("filters by fandom, rating, and completion", () => {
    expect(Array.from(runQuery(lib, query({ fandom: 1 })))).toEqual([1, 2]);
    expect(Array.from(runQuery(lib, query({ rating: 0 })))).toEqual([0, 2]);
    expect(Array.from(runQuery(lib, query({ complete: 0 })))).toEqual([1, 3]);
  });

  it("combines filters", () => {
    expect(Array.from(runQuery(lib, query({ fandom: 0, rating: 0, complete: 1 })))).toEqual([0, 2]);
    expect(Array.from(runQuery(lib, query({ fandom: 0, q: "charlie" })))).toEqual([2]);
  });

  it("orders by the requested sort", () => {
    expect(Array.from(runQuery(lib, query({ sort: "words" })))).toEqual([1, 3, 0, 2]);
    expect(Array.from(runQuery(lib, query({ sort: "title" })))).toEqual([3, 1, 0, 2]);
    expect(runQuery(lib, query({ sort: "updated" }))[0]).toBe(1);
  });

  it("returns a copy, not a view of the scratch buffer", () => {
    const first = runQuery(lib, query({ fandom: 0 }));
    const snapshot = Array.from(first);
    runQuery(lib, query({})); // would overwrite a shared view
    expect(Array.from(first)).toEqual(snapshot);
  });

  it("handles a 10k synthetic library quickly", () => {
    const big = syntheticIndex();
    const n = 10_000;
    const repeat = <T>(pick: (i: number) => T): T[] => Array.from({ length: n }, (_, i) => pick(i));
    big.count = n;
    big.works = {
      id: repeat((i) => i + 1),
      hash: repeat((i) => i.toString(16).padStart(8, "0")),
      cid: repeat((i) => i.toString(16).padStart(64, "0")),
      title: repeat((i) => `Work number ${i}`),
      authors: repeat((i) => [i % 2]),
      fandoms: repeat((i) => [i % 2]),
      series: repeat(() => []),
      tags: repeat(() => []),
      language: repeat(() => 0),
      rating: repeat((i) => (i % 2 === 0 ? 0 : 3)),
      warnings: repeat(() => 0),
      words: repeat((i) => (i * 997) % 100_000),
      chapters: repeat(() => 1),
      chapters_total: repeat(() => 1),
      complete: repeat((i) => i % 2),
      summary: repeat(() => null),
      published: repeat(() => 18_000),
      updated: repeat((i) => 18_000 + (i % 500)),
      added: repeat((i) => 1_700_000_000 + i),
      versions: repeat(() => 1),
      bytes: repeat(() => 1_000),
    };
    const bigLib = decode(big);
    const start = performance.now();
    const result = runQuery(bigLib, query({ q: "number 99", rating: 3, sort: "words" }));
    const elapsed = performance.now() - start;
    expect(result.length).toBeGreaterThan(0);
    // Generous CI-safe bound; typically ~1–3 ms.
    expect(elapsed).toBeLessThan(50);
  });
});

describe("fandomCounts", () => {
  it("counts with no filters", () => {
    expect(Array.from(fandomCounts(lib, query({})))).toEqual([3, 2]);
  });

  it("excludes the fandom filter itself (facet self-exclusion)", () => {
    const withFandom = fandomCounts(lib, query({ fandom: 1 }));
    expect(Array.from(withFandom)).toEqual([3, 2]);
  });

  it("applies the other filters", () => {
    expect(Array.from(fandomCounts(lib, query({ complete: 1 })))).toEqual([2, 1]);
  });
});
