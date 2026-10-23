import { describe, expect, it } from "vitest";
import { createStore } from "../store";
import type { ReadLogDto } from "../types/generated/ReadLogDto";
import { memoryBacked } from "./cache";
import { READS_SCHEMA, createReads, followSync, summarise } from "./reads";
import type { ReadsDeps } from "./reads";
import type { SyncState } from "./sync";

const LOG: ReadLogDto = {
  work_id: [7, 7, 7, 8],
  date: [100, 120, 110, 50],
  chapter_update: [0, 1, 1, 0],
};

interface FakeResponseInit {
  status: number;
  etag?: string;
  body?: ReadLogDto;
}

function fakeResponse({ status, etag, body }: FakeResponseInit): Response {
  const headers = new Headers();
  if (etag) headers.set("etag", etag);
  return {
    status,
    ok: status >= 200 && status < 300,
    headers,
    json: () => Promise.resolve(body),
  } as unknown as Response;
}

function harness(queue: (FakeResponseInit | Error)[] = []) {
  const calls: Record<string, string>[] = [];
  const pending = [...queue];
  const cache = memoryBacked();
  const deps: ReadsDeps = {
    fetchImpl: ((_: unknown, init?: RequestInit) => {
      calls.push((init?.headers as Record<string, string>) ?? {});
      const next = pending.shift();
      if (!next) return Promise.reject(new Error("no response queued"));
      if (next instanceof Error) return Promise.reject(next);
      return Promise.resolve(fakeResponse(next));
    }) as typeof fetch,
    cache,
  };
  return { deps, calls, cache };
}

describe("read log", () => {
  it("summarises full reads, updates and the latest date per work", () => {
    const byWork = summarise(LOG);
    expect(byWork.get(7)).toEqual({ full: 1, updates: 2, last: 120 });
    expect(byWork.get(8)).toEqual({ full: 1, updates: 0, last: 50 });
    expect(byWork.has(9)).toBe(false);
  });

  it("200 populates the store and the cache", async () => {
    const h = harness([{ status: 200, etag: '"r1"', body: LOG }]);
    const reads = createReads(h.deps);
    await reads.revalidate();
    const state = reads.readsStore.get();
    expect(state.log).toEqual(LOG);
    expect(state.byWork.get(7)?.updates).toBe(2);
    expect(state.etag).toBe('"r1"');
    expect((await h.cache.readReads())?.etag).toBe('"r1"');
  });

  it("304 keeps the log and sends If-None-Match", async () => {
    const h = harness([{ status: 200, etag: '"r1"', body: LOG }, { status: 304 }]);
    const reads = createReads(h.deps);
    await reads.revalidate();
    await reads.revalidate();
    expect(h.calls[1]?.["If-None-Match"]).toBe('"r1"');
    expect(reads.readsStore.get().log).toEqual(LOG);
  });

  it("a failure keeps the last good log and sets error", async () => {
    const h = harness([{ status: 200, etag: '"r1"', body: LOG }, { status: 500 }, new Error("offline")]);
    const reads = createReads(h.deps);
    await reads.revalidate();
    await reads.revalidate();
    expect(reads.readsStore.get().error).toContain("500");
    await reads.revalidate();
    expect(reads.readsStore.get().error).toContain("offline");
    expect(reads.readsStore.get().log).toEqual(LOG);
  });

  it("a failure with no log leaves every work unread", async () => {
    const h = harness([{ status: 500 }]);
    const reads = createReads(h.deps);
    await expect(reads.revalidate()).resolves.toBeUndefined();
    expect(reads.readsStore.get().log).toBeNull();
    expect(reads.readsStore.get().byWork.size).toBe(0);
  });

  it("boot loads the cached log without any network", async () => {
    const h = harness();
    await h.cache.writeReads({ schema: READS_SCHEMA, etag: '"r1"', log: LOG });
    const reads = createReads(h.deps);
    await reads.boot();
    expect(reads.readsStore.get().byWork.get(8)?.full).toBe(1);
    expect(h.calls).toHaveLength(0);
  });

  it("boot ignores a cached copy with another schema", async () => {
    const h = harness();
    await h.cache.writeReads({ schema: READS_SCHEMA + 1, etag: '"r1"', log: LOG });
    const reads = createReads(h.deps);
    await reads.boot();
    expect(reads.readsStore.get().log).toBeNull();
  });

  it("follows successful index checks only", async () => {
    const h = harness([
      { status: 200, etag: '"r1"', body: LOG },
      { status: 304 },
    ]);
    const reads = createReads(h.deps);
    const syncStore = createStore({ checkedAt: null } as SyncState);
    followSync(reads, syncStore);
    syncStore.patch({ error: "offline" });
    expect(h.calls).toHaveLength(0);
    syncStore.patch({ checkedAt: 1 });
    await reads.revalidate();
    syncStore.patch({ checkedAt: 1, refreshing: true });
    syncStore.patch({ checkedAt: 2 });
    await reads.revalidate();
    expect(h.calls).toHaveLength(2);
  });
});
