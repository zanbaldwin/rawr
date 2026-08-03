import { describe, expect, it } from "vitest";
import type { LibraryIndex } from "../types/generated/LibraryIndex";
import { DecodeError, SCHEMA, assertIndexPayload, decode, titlePermutation } from "./decode";

/** A small but structurally complete payload. */
export function syntheticIndex(): LibraryIndex {
  return {
    format: 1,
    snapshot: "abc123",
    generated_at: 1_800_000_000,
    target: "local",
    count: 4,
    enums: {
      ratings: [
        { code: 0, short: "G", label: "General Audiences" },
        { code: 3, short: "E", label: "Explicit" },
        { code: 5, short: "?", label: "Unknown" },
      ],
      warnings: [{ code: 1, short: "none", label: "No Archive Warnings Apply" }],
      tag_kinds: [
        { code: 0, short: "relationship", label: "Relationship" },
        { code: 2, short: "freeform", label: "Freeform" },
      ],
    },
    dict: {
      authors: [
        { u: "alpha", p: null },
        { u: "beta", p: "bee" },
      ],
      fandoms: ["Fandom One", "Fandom Two"],
      languages: [{ name: "English", iso: "en" }],
      series: [{ id: 7, name: "The Series" }],
      tags: ["Fluff", "Angst", "Coffee"],
      tag_kinds: [2, 2, 2],
    },
    works: {
      id: [100, 200, 300, 400],
      hash: ["00000064", "000000c8", "0000012c", "00000190"],
      cid: ["c".repeat(64), "d".repeat(64), "e".repeat(64), "f".repeat(64)],
      title: ["Beta Work", "alpha work", "Charlie Work", "10 Things"],
      authors: [[0], [1], [0, 1], []],
      fandoms: [[0], [1], [0, 1], [0]],
      series: [[{ i: 0, pos: 1 }], [], [], []],
      tags: [[0, 1], [2], [], [0]],
      language: [0, 0, 0, 0],
      rating: [0, 3, 0, 5],
      warnings: [1, 0, 1, 0],
      words: [1000, 50_000, 200, 7_000],
      chapters: [1, 10, 2, 3],
      chapters_total: [1, -1, 2, 5],
      complete: [1, 0, 1, 0],
      summary: ["A summary", null, "Another", null],
      published: [18_000, 18_100, 18_200, 18_300],
      updated: [18_050, 19_000, 18_200, 18_400],
      added: [1_700_000_000, 1_700_000_100, 1_700_000_200, 1_700_000_300],
      versions: [1, 2, 1, 1],
      bytes: [10_000, 500_000, 2_000, 70_000],
    },
  };
}

describe("assertIndexPayload", () => {
  it("accepts the synthetic payload", () => {
    expect(() => assertIndexPayload(syntheticIndex())).not.toThrow();
  });

  it("rejects an unknown format", () => {
    const index = syntheticIndex();
    index.format = 99;
    expect(() => assertIndexPayload(index)).toThrow(DecodeError);
  });

  it("rejects a short column", () => {
    const index = syntheticIndex();
    index.works.words = [1, 2, 3];
    expect(() => assertIndexPayload(index)).toThrow(/words/);
  });

  it("rejects an out-of-range dictionary reference", () => {
    const index = syntheticIndex();
    index.works.tags[1] = [17];
    expect(() => assertIndexPayload(index)).toThrow(/tags reference 17/);
  });
});

describe("decode", () => {
  it("builds CSR with exact boundaries at rows 0 and N-1", () => {
    const lib = decode(syntheticIndex());
    expect(lib.schema).toBe(SCHEMA);
    expect(Array.from(lib.tags.offsets)).toEqual([0, 2, 3, 3, 4]);
    // Row 0.
    expect(Array.from(lib.tags.values.subarray(lib.tags.offsets[0]!, lib.tags.offsets[1]!))).toEqual([0, 1]);
    // Empty middle row.
    expect(lib.tags.offsets[2]).toBe(lib.tags.offsets[3]);
    // Last row.
    expect(Array.from(lib.tags.values.subarray(lib.tags.offsets[3]!, lib.tags.offsets[4]!))).toEqual([0]);
    // Series CSR carries positions.
    expect(lib.series.positions[0]).toBe(1);
    // Lowercase projections exist.
    expect(lib.titleLower[1]).toBe("alpha work");
    expect(lib.dict.authorsSearch[1]).toBe("beta bee");
  });

  it("precomputes permutations that are true permutations", () => {
    const lib = decode(syntheticIndex());
    for (const perm of [lib.perm.recent, lib.perm.words, lib.perm.updated, titlePermutation(lib)]) {
      expect(Array.from(perm).sort((a, b) => a - b)).toEqual([0, 1, 2, 3]);
    }
    // Words descending: 50k, 7k, 1k, 200.
    expect(Array.from(lib.perm.words)).toEqual([1, 3, 0, 2]);
    // Updated descending: row1 (19000) first.
    expect(lib.perm.updated[0]).toBe(1);
    // Recent = wire order.
    expect(Array.from(lib.perm.recent)).toEqual([0, 1, 2, 3]);
    // Title: numeric-aware, case-insensitive → "10 Things", alpha, Beta, Charlie.
    expect(Array.from(titlePermutation(lib))).toEqual([3, 1, 0, 2]);
  });
});
