// The persistence port: two records in IndexedDB ('index' — the *decoded*
// Library, typed arrays structured-clone near-memcpy; 'meta' — sync
// bookkeeping). Falls back to memory-only when IndexedDB won't open, so
// the app degrades to "always first-run" rather than a white screen.

import type { Library } from "../types";
import { deleteDb, kvGet, kvPut, openDb } from "./idb";

export interface CachedMeta {
  schema: number;
  etag: string;
  /** Unix seconds of the last 200 (data actually replaced). */
  fetchedAt: number;
  /** Unix seconds of the last successful check (200 or 304). */
  checkedAt: number;
}

export interface CacheStore {
  read: () => Promise<{ library: Library; meta: CachedMeta } | null>;
  write: (library: Library, meta: CachedMeta) => Promise<void>;
  writeMeta: (meta: CachedMeta) => Promise<void>;
  clear: () => Promise<void>;
}

const INDEX_KEY = "index";
const META_KEY = "meta";

export function createCache(): CacheStore {
  const backend: Promise<CacheStore> = openDb().then(
    (db) => idbBacked(db),
    () => memoryBacked(),
  );
  return {
    read: () => backend.then((b) => b.read()),
    write: (library, meta) => backend.then((b) => b.write(library, meta)),
    writeMeta: (meta) => backend.then((b) => b.writeMeta(meta)),
    clear: () => backend.then((b) => b.clear()),
  };
}

function idbBacked(db: IDBDatabase): CacheStore {
  return {
    async read() {
      const [library, meta] = await Promise.all([
        kvGet<Library>(db, INDEX_KEY),
        kvGet<CachedMeta>(db, META_KEY),
      ]);
      if (!library || !meta) return null;
      return { library, meta };
    },
    async write(library, meta) {
      // Two puts, not atomic — acceptable: a torn write is caught by the
      // schema/etag checks and simply refetched.
      await kvPut(db, INDEX_KEY, library);
      await kvPut(db, META_KEY, meta);
    },
    async writeMeta(meta) {
      await kvPut(db, META_KEY, meta);
    },
    async clear() {
      db.close();
      await deleteDb();
    },
  };
}

export function memoryBacked(): CacheStore {
  let stored: { library: Library; meta: CachedMeta } | null = null;
  return {
    read: () => Promise.resolve(stored),
    write(library, meta) {
      stored = { library, meta };
      return Promise.resolve();
    },
    writeMeta(meta) {
      if (stored) stored = { ...stored, meta };
      return Promise.resolve();
    },
    clear() {
      stored = null;
      return Promise.resolve();
    },
  };
}
