// The sync engine. Dependencies arrive as parameters (never module
// imports), which is what keeps this DOM-free and testable.
//
// States: booting → (cached? ready : loading) → ready | failed, with
// ready → refreshing → ready for revalidation. `ready` is the only state
// with data, and **the index is never discarded on a failed refresh** —
// `online`, `error` and age are attributes of ready, not separate states.

import { createStore } from "../store";
import type { Store } from "../store";
import type { Library } from "../types";
import type { LibraryIndex } from "../types/generated/LibraryIndex";
import type { CacheStore } from "./cache";
import { SCHEMA, decode } from "./decode";

export interface SyncState {
  status: "booting" | "loading" | "ready" | "failed";
  refreshing: boolean;
  /** Derived from the last request outcome — never `navigator.onLine`. */
  online: boolean;
  etag: string | null;
  fetchedAt: number | null;
  checkedAt: number | null;
  /** Download progress: decompressed bytes so far / X-Index-Length. */
  received: number;
  total: number | null;
  error: string | null;
}

const INITIAL: SyncState = {
  status: "booting",
  refreshing: false,
  online: false,
  etag: null,
  fetchedAt: null,
  checkedAt: null,
  received: 0,
  total: null,
  error: null,
};

export interface SyncDeps {
  fetchImpl: typeof fetch;
  cache: CacheStore;
  /** Milliseconds, injectable for tests. */
  now: () => number;
}

export interface Sync {
  syncStore: Store<SyncState>;
  libraryStore: Store<Library | null>;
  boot: () => Promise<void>;
  /** Coalesced; respects the staleness window unless forced. */
  revalidate: (opts?: { force?: boolean }) => Promise<void>;
  /** The manual button: bypasses the window AND the server's tier-1
   * change detection (`cache: 'reload'` sends Cache-Control: no-cache). */
  refresh: () => Promise<void>;
  /** Abort any in-flight request (app going to background). */
  abort: () => void;
  clear: () => Promise<void>;
}

/** Don't re-check within a minute: iOS fires visibilitychange repeatedly
 * while the user scrubs the app switcher. */
const MIN_INTERVAL_S = 60;
/** Revalidation with cached data races an 8 s timeout; a first-run
 * download gets no timeout. */
const REVALIDATE_TIMEOUT_MS = 8_000;

export function createSync(deps: SyncDeps): Sync {
  const syncStore = createStore<SyncState>(INITIAL);
  const libraryStore = createStore<Library | null>(null);
  let inFlight: Promise<void> | null = null;
  let controller: AbortController | null = null;

  async function boot(): Promise<void> {
    const cached = await deps.cache.read().catch(() => null);
    if (cached && cached.library.schema === SCHEMA) {
      libraryStore.set(cached.library);
      syncStore.patch({
        status: "ready",
        etag: cached.meta.etag,
        fetchedAt: cached.meta.fetchedAt,
        checkedAt: cached.meta.checkedAt,
      });
    } else {
      // Missing or schema-bumped cache: refetch is the migration strategy.
      syncStore.patch({ status: "loading" });
    }
    // Un-awaited: first paint happens off the cache, revalidation races on.
    void revalidate({ force: true });
  }

  function revalidate(opts: { force?: boolean } = {}): Promise<void> {
    if (inFlight) return inFlight;
    const state = syncStore.get();
    const fresh =
      state.checkedAt !== null && deps.now() / 1000 - state.checkedAt < MIN_INTERVAL_S;
    if (!opts.force && fresh) return Promise.resolve();
    inFlight = doSync(Boolean(opts.force)).finally(() => {
      inFlight = null;
    });
    return inFlight;
  }

  async function doSync(force: boolean): Promise<void> {
    controller = new AbortController();
    const hasData = libraryStore.get() !== null;
    const etag = syncStore.get().etag;
    const headers: Record<string, string> = {};
    if (hasData && etag) headers["If-None-Match"] = etag;
    syncStore.patch({ refreshing: true, received: 0, total: null });
    const signal =
      hasData && "any" in AbortSignal
        ? AbortSignal.any([controller.signal, AbortSignal.timeout(REVALIDATE_TIMEOUT_MS)])
        : controller.signal;
    try {
      const response = await deps.fetchImpl("/api/v1/index", {
        headers,
        // Routine: no-store stops the HTTP cache handing back an opaque
        // 200 that would silently break age tracking. Forced: reload
        // sends Cache-Control: no-cache — the server's rebuild bypass.
        cache: force && hasData ? "reload" : "no-store",
        signal,
      });
      const nowS = Math.round(deps.now() / 1000);
      if (response.status === 304) {
        syncStore.patch({ status: "ready", refreshing: false, online: true, checkedAt: nowS, error: null });
        const meta = currentMeta(nowS);
        if (meta) void deps.cache.writeMeta(meta);
        return;
      }
      if (!response.ok) throw new Error(`server answered ${response.status}`);
      const text = await readBody(response, (received, total) => {
        syncStore.patch({ received, total });
      });
      const payload = JSON.parse(text) as LibraryIndex;
      const library = decode(payload);
      const newEtag = response.headers.get("etag") ?? "";
      libraryStore.set(library);
      syncStore.patch({
        status: "ready",
        refreshing: false,
        online: true,
        etag: newEtag,
        fetchedAt: nowS,
        checkedAt: nowS,
        error: null,
      });
      await deps.cache.write(library, { schema: SCHEMA, etag: newEtag, fetchedAt: nowS, checkedAt: nowS });
    } catch (error) {
      const stillHasData = libraryStore.get() !== null;
      syncStore.patch({
        status: stillHasData ? "ready" : "failed",
        refreshing: false,
        online: false,
        error: String(error),
      });
    } finally {
      controller = null;
    }
  }

  function currentMeta(checkedAt: number): { schema: number; etag: string; fetchedAt: number; checkedAt: number } | null {
    const state = syncStore.get();
    if (state.etag === null || state.fetchedAt === null) return null;
    return { schema: SCHEMA, etag: state.etag, fetchedAt: state.fetchedAt, checkedAt };
  }

  return {
    syncStore,
    libraryStore,
    boot,
    revalidate,
    refresh: () => revalidate({ force: true }),
    abort: () => controller?.abort(),
    clear: async () => {
      await deps.cache.clear();
      libraryStore.set(null);
      syncStore.set(INITIAL);
    },
  };
}

/** Stream the body for progress when possible; fall back to text(). */
async function readBody(response: Response, onProgress: (received: number, total: number | null) => void): Promise<string> {
  const total = Number(response.headers.get("x-index-length")) || null;
  const body = response.body;
  if (!body || typeof body.getReader !== "function") {
    return response.text();
  }
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let received = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    received += value.byteLength;
    onProgress(received, total);
  }
  const merged = new Uint8Array(received);
  let cursor = 0;
  for (const chunk of chunks) {
    merged.set(chunk, cursor);
    cursor += chunk.byteLength;
  }
  return new TextDecoder().decode(merged);
}

/** Browser trigger wiring — DOM-touching, kept out of the testable core.
 * Call once from main. */
export function attachSyncTriggers(sync: Sync): void {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
      void sync.revalidate();
    } else {
      // iOS suspends the process; a request surviving into the freezer
      // comes back as a stuck spinner. Start fresh on foreground.
      sync.abort();
    }
  });
  // bfcache restores don't fire visibilitychange reliably.
  window.addEventListener("pageshow", (event) => {
    if (event.persisted) void sync.revalidate();
  });
  // A fast positive only — the request itself is the real probe.
  window.addEventListener("online", () => void sync.revalidate());
}
