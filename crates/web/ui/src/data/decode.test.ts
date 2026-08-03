import { describe, expect, it } from "vitest";
import { DecodeError, SCHEMA, assertIndexPayload, decode, titlePermutation } from "./decode";
import { syntheticIndex } from "./fixture";


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
