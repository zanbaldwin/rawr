import { describe, expect, it } from "vitest";
import { parseHash, routeToHash } from "./router";

describe("parseHash", () => {
  it("parses the core routes", () => {
    expect(parseHash("").route).toEqual({ name: "list" });
    expect(parseHash("#/").route).toEqual({ name: "list" });
    expect(parseHash("#/works/12345").route).toEqual({ name: "work", id: 12345 });
    expect(parseHash("#/upload").route).toEqual({ name: "upload" });
    expect(parseHash("#/stats").route).toEqual({ name: "stats" });
    expect(parseHash("#/about").route).toEqual({ name: "about" });
  });

  it("keeps filter params attached to the list route", () => {
    const state = parseHash("#/?q=coffee&fandom=Naruto&rating=g&complete=1&sort=words");
    expect(state.route).toEqual({ name: "list" });
    expect(state.params.get("q")).toBe("coffee");
    expect(state.params.get("fandom")).toBe("Naruto");
    expect(state.params.get("sort")).toBe("words");
  });

  it("never throws on junk", () => {
    for (const junk of ["#/works/", "#/works/abc", "#/works/-3", "#/works/1/2", "#/nope", "#/upload/extra", "#/%%%"]) {
      expect(parseHash(junk).route.name).toBe("notfound");
    }
    // Junk QUERY is preserved but harmless.
    expect(parseHash("#/?rating=zzz&sort=&complete=maybe").route.name).toBe("list");
  });

  it("round-trips route → hash → route", () => {
    const params = new URLSearchParams({ q: "tea", sort: "title" });
    const hash = routeToHash({ name: "list" }, params);
    const back = parseHash(hash);
    expect(back.route).toEqual({ name: "list" });
    expect(back.params.get("q")).toBe("tea");
    expect(parseHash(routeToHash({ name: "work", id: 77 })).route).toEqual({ name: "work", id: 77 });
  });
});
