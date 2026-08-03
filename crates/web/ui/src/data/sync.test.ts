import { describe, expect, it } from "vitest";
import { memoryBacked } from "./cache";
import { SCHEMA, decode } from "./decode";
import { syntheticIndex } from "./fixture";
import { createSync } from "./sync";
import type { SyncDeps } from "./sync";

interface FakeResponseInit {
  status: number;
  etag?: string;
  body?: string;
}

function fakeResponse({ status, etag, body }: FakeResponseInit): Response {
  const headers = new Headers();
  if (etag) headers.set("etag", etag);
  if (body) headers.set("x-index-length", String(body.length));
  return {
    status,
    ok: status >= 200 && status < 300,
    headers,
    body: null,
    text: () => Promise.resolve(body ?? ""),
  } as unknown as Response;
}

function indexBody(): string {
  return JSON.stringify(syntheticIndex());
}

interface Harness {
  deps: SyncDeps;
  calls: { headers: Record<string, string>; cache: string }[];
  respond: (init: FakeResponseInit | Error) => void;
  clock: { now: number };
}

function harness(queue: (FakeResponseInit | Error)[] = []): Harness {
  const calls: Harness["calls"] = [];
  const pending = [...queue];
  const clock = { now: 1_800_000_000_000 };
  const deps: SyncDeps = {
    fetchImpl: ((_: unknown, init?: RequestInit) => {
      calls.push({
        headers: (init?.headers as Record<string, string>) ?? {},
        cache: String(init?.cache),
      });
      const next = pending.shift();
      if (!next) return Promise.reject(new Error("no response queued"));
      if (next instanceof Error) return Promise.reject(next);
      return Promise.resolve(fakeResponse(next));
    }) as typeof fetch,
    cache: memoryBacked(),
    now: () => clock.now,
  };
  return { deps, calls, respond: (r) => pending.push(r), clock };
}

describe("sync engine", () => {
  it("first run: 200 populates the library and the cache", async () => {
    const h = harness([{ status: 200, etag: '"abc-br"', body: indexBody() }]);
    const sync = createSync(h.deps);
    await sync.boot();
    await sync.revalidate();
    expect(sync.syncStore.get().status).toBe("ready");
    expect(sync.syncStore.get().online).toBe(true);
    expect(sync.syncStore.get().etag).toBe('"abc-br"');
    expect(sync.libraryStore.get()?.count).toBe(4);
    const cached = await h.deps.cache.read();
    expect(cached?.meta.etag).toBe('"abc-br"');
  });

  it("304 moves checkedAt but not fetchedAt, and sends If-None-Match", async () => {
    const h = harness([{ status: 200, etag: '"abc-br"', body: indexBody() }, { status: 304 }]);
    const sync = createSync(h.deps);
    await sync.boot();
    await sync.revalidate(); // join boot's un-awaited initial sync
    const fetchedAt = sync.syncStore.get().fetchedAt;
    h.clock.now += 120_000;
    await sync.refresh();
    expect(h.calls[1]?.headers["If-None-Match"]).toBe('"abc-br"');
    expect(sync.syncStore.get().fetchedAt).toBe(fetchedAt);
    expect(sync.syncStore.get().checkedAt).toBe(Math.round(h.clock.now / 1000));
  });

  it("a failed refresh keeps the data: ready, offline, error set", async () => {
    const h = harness([{ status: 200, etag: '"abc-br"', body: indexBody() }, new Error("network down")]);
    const sync = createSync(h.deps);
    await sync.boot();
    await sync.revalidate(); // join boot's un-awaited initial sync
    h.clock.now += 120_000;
    await sync.refresh();
    const state = sync.syncStore.get();
    expect(state.status).toBe("ready");
    expect(state.online).toBe(false);
    expect(state.error).toContain("network down");
    expect(sync.libraryStore.get()?.count).toBe(4);
  });

  it("a failed first run goes to failed", async () => {
    const h = harness([new Error("no route to host")]);
    const sync = createSync(h.deps);
    await sync.boot();
    expect(sync.syncStore.get().status).toBe("failed");
    expect(sync.libraryStore.get()).toBeNull();
  });

  it("boot renders from a cached copy before any network", async () => {
    const h = harness([new Error("nas asleep")]);
    const library = decode(syntheticIndex());
    await h.deps.cache.write(library, { schema: SCHEMA, etag: '"abc-br"', fetchedAt: 1, checkedAt: 1 });
    const sync = createSync(h.deps);
    await sync.boot();
    // Library present even though revalidation failed.
    expect(sync.libraryStore.get()?.count).toBe(4);
    expect(sync.syncStore.get().status).toBe("ready");
    expect(sync.syncStore.get().online).toBe(false);
  });

  it("a schema bump discards the cached copy", async () => {
    const h = harness([{ status: 200, etag: '"new-br"', body: indexBody() }]);
    const library = decode(syntheticIndex());
    const stale = { ...library, schema: SCHEMA - 1 };
    await h.deps.cache.write(stale, { schema: SCHEMA - 1, etag: '"old-br"', fetchedAt: 1, checkedAt: 1 });
    const sync = createSync(h.deps);
    await sync.boot();
    await sync.revalidate();
    // No If-None-Match: the stale cache must not be revalidated.
    expect(h.calls[0]?.headers["If-None-Match"]).toBeUndefined();
    expect(sync.syncStore.get().etag).toBe('"new-br"');
  });

  it("two concurrent revalidates issue one fetch", async () => {
    const h = harness([{ status: 200, etag: '"abc-br"', body: indexBody() }]);
    const sync = createSync(h.deps);
    await Promise.all([sync.revalidate({ force: true }), sync.revalidate({ force: true })]);
    expect(h.calls.length).toBe(1);
  });

  it("respects the staleness window unless forced", async () => {
    const h = harness([
      { status: 200, etag: '"abc-br"', body: indexBody() },
      { status: 304 },
      { status: 304 },
    ]);
    const sync = createSync(h.deps);
    await sync.boot();
    await sync.revalidate(); // within 60 s of boot's check — no fetch
    expect(h.calls.length).toBe(1);
    await sync.refresh(); // forced — fetches
    expect(h.calls.length).toBe(2);
    h.clock.now += 120_000;
    await sync.revalidate(); // stale now — fetches
    expect(h.calls.length).toBe(3);
  });

  it("the manual path uses cache:reload, the routine path no-store", async () => {
    const h = harness([
      { status: 200, etag: '"abc-br"', body: indexBody() },
      { status: 304 },
      { status: 304 },
    ]);
    const sync = createSync(h.deps);
    await sync.boot(); // first run: no data yet → no-store even though forced
    await sync.revalidate(); // join boot's un-awaited initial sync
    expect(h.calls[0]?.cache).toBe("no-store");
    await sync.refresh();
    expect(h.calls[1]?.cache).toBe("reload");
    h.clock.now += 120_000;
    await sync.revalidate();
    expect(h.calls[2]?.cache).toBe("no-store");
  });
});
