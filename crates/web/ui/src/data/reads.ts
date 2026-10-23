// The read log: a small companion to the index, synced on its own ETag.
// Like the sync engine, dependencies arrive as parameters.
//
// Read tracking is optional. A failed fetch (offline, or a 500 because
// the log file is invalid) keeps the last good log and only sets
// `error`; with no log at all, every work simply reads as unread.

import { createStore } from "../store";
import type { Store } from "../store";
import type { ReadEventDto } from "../types/generated/ReadEventDto";
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
  /** Append a read on the server, then to the local log. Throws with
   * the server's message on failure. */
  record: (event: ReadEventDto) => Promise<void>;
  /** Remove the last matching read on the server, then locally. */
  undo: (event: ReadEventDto) => Promise<void>;
}

/** Today's date in the reader's own time zone, as `YYYY-MM-DD`. */
export function localToday(now = new Date()): string {
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

/** `YYYY-MM-DD` → days since the Unix epoch, matching the wire format. */
export function isoToDays(iso: string): number {
  const [year, month, day] = iso.split("-").map(Number);
  return Date.UTC(year!, month! - 1, day!) / 86_400_000;
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

  /** Apply a confirmed change to the local log. The stored ETag no
   * longer matches the server's, so the next check refetches. */
  function change(update: (log: ReadLogDto) => ReadLogDto): void {
    const state = readsStore.get();
    const log = update(state.log ?? { work_id: [], date: [], chapter_update: [] });
    readsStore.set({ ...state, log, byWork: summarise(log), error: null });
  }

  async function send(method: "POST" | "DELETE", event: ReadEventDto): Promise<void> {
    const response = await deps.fetchImpl("/api/v1/reads", {
      method,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(event),
      cache: "no-store",
    });
    if (response.ok) return;
    const body = (await response.json().catch(() => null)) as { message?: string } | null;
    throw new Error(body?.message ?? `server answered ${response.status}`);
  }

  async function record(event: ReadEventDto): Promise<void> {
    await send("POST", event);
    change((log) => ({
      work_id: [...log.work_id, event.work_id],
      date: [...log.date, isoToDays(event.date)],
      chapter_update: [...log.chapter_update, event.chapter_update ? 1 : 0],
    }));
  }

  async function undo(event: ReadEventDto): Promise<void> {
    await send("DELETE", event);
    const date = isoToDays(event.date);
    const flag = event.chapter_update ? 1 : 0;
    change((log) => {
      let index = log.work_id.length - 1;
      while (index >= 0 && !(log.work_id[index] === event.work_id && log.date[index] === date && log.chapter_update[index] === flag)) {
        index--;
      }
      if (index < 0) return log;
      const without = <T,>(values: T[]) => values.filter((_, i) => i !== index);
      return { work_id: without(log.work_id), date: without(log.date), chapter_update: without(log.chapter_update) };
    });
  }

  function revalidate(): Promise<void> {
    if (inFlight) return inFlight;
    inFlight = doFetch().finally(() => {
      inFlight = null;
    });
    return inFlight;
  }

  return { readsStore, boot, revalidate, record, undo };
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
