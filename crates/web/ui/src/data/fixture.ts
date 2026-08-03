// Shared synthetic payload for tests. Lives outside the .test files so
// importing it does not re-register another file's test suites.

import type { LibraryIndex } from "../types/generated/LibraryIndex";

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
