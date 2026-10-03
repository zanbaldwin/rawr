import { describe, expect, it } from "vitest";
import { decode } from "../data/decode";
import { syntheticIndex } from "../data/fixture";
import { paramsToQuery, queryToParams } from "./useListQuery";

const lib = decode(syntheticIndex());

describe("params ↔ query", () => {
  it("round-trips multiple fandoms by name", () => {
    const params = new URLSearchParams();
    params.append("fandom", "Fandom One");
    params.append("fandom", "Fandom Two");
    params.set("lang", "DE");
    const query = paramsToQuery(lib, params);
    expect(query.fandoms).toEqual([0, 1]);
    expect(query.language).toBe(1);
    const back = queryToParams(lib, query);
    expect(back.getAll("fandom")).toEqual(["Fandom One", "Fandom Two"]);
    expect(back.get("lang")).toBe("de");
  });

  it("drops fandom names that no longer resolve", () => {
    const params = new URLSearchParams();
    params.append("fandom", "Fandom One");
    params.append("fandom", "Renamed Away");
    expect(paramsToQuery(lib, params).fandoms).toEqual([0]);
  });

  it("tolerates junk everywhere", () => {
    const params = new URLSearchParams("lang=zzz&complete=maybe&sort=sideways&q=%20%20");
    const query = paramsToQuery(lib, params);
    expect(query).toEqual({ q: "", fandoms: [], language: -1, complete: -1, sort: "recent" });
    // Defaults produce a clean URL.
    expect(queryToParams(lib, query).toString()).toBe("");
  });
});
