// Thin promise wrappers over one IndexedDB database, one object store.
// The entire surface is three gets and two puts, which is why this is
// forty lines and not a dependency. Every request is issued
// synchronously within its transaction — the auto-close footgun cannot
// fire.

const DB_NAME = "rawr";
const DB_VERSION = 1;
const STORE = "kv";

/** Safari has been seen hanging `indexedDB.open` after a crashed tab;
 * the race means the app degrades to memory-only instead of a white
 * screen. */
export function openDb(timeoutMs = 3000): Promise<IDBDatabase> {
  const open = new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      request.result.createObjectStore(STORE);
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("indexeddb open failed"));
    request.onblocked = () => reject(new Error("indexeddb open blocked"));
  });
  const timeout = new Promise<never>((_, reject) => {
    setTimeout(() => reject(new Error("indexeddb open timed out")), timeoutMs);
  });
  return Promise.race([open, timeout]);
}

export function kvGet<T>(db: IDBDatabase, key: string): Promise<T | undefined> {
  return new Promise((resolve, reject) => {
    const request = db.transaction(STORE, "readonly").objectStore(STORE).get(key);
    request.onsuccess = () => resolve(request.result as T | undefined);
    request.onerror = () => reject(request.error ?? new Error("indexeddb get failed"));
  });
}

export function kvPut(db: IDBDatabase, key: string, value: unknown): Promise<void> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE, "readwrite");
    tx.objectStore(STORE).put(value, key);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error("indexeddb put failed"));
    tx.onabort = () => reject(tx.error ?? new Error("indexeddb put aborted"));
  });
}

export function deleteDb(): Promise<void> {
  return new Promise((resolve) => {
    const request = indexedDB.deleteDatabase(DB_NAME);
    request.onsuccess = () => resolve();
    // Deletion failing is not worth blocking "clear cache" over.
    request.onerror = () => resolve();
    request.onblocked = () => resolve();
  });
}
