// The read log: a small companion to the index, synced on its own ETag.
// Like the sync engine, dependencies arrive as parameters.
//
// Read tracking is optional. A failed fetch (offline, or a 500 because
// the log file is invalid) keeps the last good log and only sets
// `error`; with no log at all, every work simply reads as unread.

import { createStore } from "../store";
import type { Store } from "../store";
import type { ReadLogDto } from "../types/generated/ReadLogDto";
import type { CacheStore } from "./cache";
import type { SyncState } from "./sync";

/** Bump when CachedReads changes shape; a mismatch discards the copy. */
export const READS_SCHEMA = 1;

export interface ReadSummary {
  full: number;
  updates: number;
  /** Days since the Unix epoch of the most recent read of either kind. */
  last: number;
}

export interface ReadsState {
  /** `null` until a log has been loaded from the cache or the server. */
  log: ReadLogDto | null;
  byWork: Map<number, ReadSummary>;
  etag: string | null;
  error: string | null;
}

const INITIAL: ReadsState = { log: null, byWork: new Map(), etag: null, error: null };

export interface ReadsDeps {
  fetchImpl: typeof fetch;
  cache: Pick<CacheStore, "readReads" | "writeReads">;
}

export interface Reads {
  readsStore: Store<ReadsState>;
  /** Load the cached log; never touches the network. */
  boot: () => Promise<void>;
  /** Coalesced conditional fetch. Never throws. */
  revalidate: () => Promise<void>;
}

export function summarise(log: ReadLogDto): Map<number, ReadSummary> {
  const byWork = new Map<number, ReadSummary>();
  for (let i = 0; i < log.work_id.length; i++) {
    const workId = log.work_id[i]!;
    const date = log.date[i]!;
    const summary = byWork.get(workId) ?? { full: 0, updates: 0, last: date };
    if (log.chapter_update[i]) summary.updates++;
    else summary.full++;
    summary.last = Math.max(summary.last, date);
    byWork.set(workId, summary);
  }
  return byWork;
}

export function createReads(deps: ReadsDeps): Reads {
  const readsStore = createStore<ReadsState>(INITIAL);
  let inFlight: Promise<void> | null = null;

  function accept(log: ReadLogDto, etag: string): void {
    readsStore.set({ log, byWork: summarise(log), etag, error: null });
  }

  async function boot(): Promise<void> {
    const cached = await deps.cache.readReads().catch(() => null);
    if (cached && cached.schema === READS_SCHEMA && readsStore.get().log === null) {
      accept(cached.log, cached.etag);
    }
  }

  async function doFetch(): Promise<void> {
    const { log, etag } = readsStore.get();
    const headers: Record<string, string> = {};
    if (log && etag) headers["If-None-Match"] = etag;
    try {
      const response = await deps.fetchImpl("/api/v1/reads", { headers, cache: "no-store" });
      if (response.status === 304) {
        readsStore.patch({ error: null });
        return;
      }
      if (!response.ok) throw new Error(`server answered ${response.status}`);
      const next = (await response.json()) as ReadLogDto;
      const nextEtag = response.headers.get("etag") ?? "";
      accept(next, nextEtag);
      await deps.cache.writeReads({ schema: READS_SCHEMA, etag: nextEtag, log: next }).catch(() => undefined);
    } catch (error) {
      readsStore.patch({ error: String(error) });
    }
  }

  function revalidate(): Promise<void> {
    if (inFlight) return inFlight;
    inFlight = doFetch().finally(() => {
      inFlight = null;
    });
    return inFlight;
  }

  return { readsStore, boot, revalidate };
}

/** Revalidate the read log whenever the index check succeeds, so it
 * shares the index's triggers, staleness window and manual refresh. */
export function followSync(reads: Reads, syncStore: Store<SyncState>): () => void {
  let last = syncStore.get().checkedAt;
  return syncStore.subscribe(() => {
    const { checkedAt } = syncStore.get();
    if (checkedAt === null || checkedAt === last) return;
    last = checkedAt;
    void reads.revalidate();
  });
}
