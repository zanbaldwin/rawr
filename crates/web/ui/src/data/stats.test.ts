import { describe, expect, it } from "vitest";
import { decode } from "./decode";
import { syntheticIndex } from "./fixture";
import { computeReadStats } from "./stats";

const lib = decode(syntheticIndex());

describe("computeReadStats", () => {
  it("is null without a read log", () => {
    expect(computeReadStats(lib, new Map())).toBeNull();
  });

  it("counts only works in the library", () => {
    const byWork = new Map([
      [100, { full: 2, updates: 1, last: 10 }],
      [300, { full: 0, updates: 4, last: 20 }],
      [999, { full: 5, updates: 5, last: 30 }],
    ]);
    expect(computeReadStats(lib, byWork)).toEqual({ works: 2, unread: 2, full: 2, updates: 5 });
  });
});
