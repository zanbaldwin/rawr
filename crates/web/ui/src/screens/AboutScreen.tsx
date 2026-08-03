import { useEffect, useState } from "react";
import { sync } from "../data/instance";
import { formatAge, formatBytes, formatNumber } from "../format";
import { useLibrary, useSyncState } from "../hooks/useSync";

/** Sync state, storage, and the escape hatch for "it's showing stale
 * data": clear everything and refetch. */
export function AboutScreen() {
  const state = useSyncState();
  const library = useLibrary();
  const [estimate, setEstimate] = useState<StorageEstimate | null>(null);

  useEffect(() => {
    navigator.storage
      ?.estimate?.()
      .then(setEstimate)
      .catch(() => setEstimate(null));
  }, []);

  const clearAndRefetch = async () => {
    await sync.clear();
    if ("caches" in window) {
      for (const key of await caches.keys()) await caches.delete(key);
    }
    location.reload();
  };

  return (
    <section className="about">
      <h1>About</h1>
      <dl className="meta-table">
        <dt>Library</dt>
        <dd>{library ? `${formatNumber(library.count)} works (target “${library.target}”)` : "not downloaded"}</dd>
        <dt>Synced</dt>
        <dd>{state.fetchedAt ? formatAge(state.fetchedAt) : "never"}</dd>
        <dt>Checked</dt>
        <dd>{state.checkedAt ? formatAge(state.checkedAt) : "never"}</dd>
        <dt>Snapshot</dt>
        <dd>
          <code>{state.etag ?? "—"}</code>
        </dd>
        <dt>Server</dt>
        <dd>{state.online ? "reachable" : "offline"}</dd>
        <dt>Storage</dt>
        <dd>
          {estimate?.usage !== undefined
            ? `${formatBytes(estimate.usage)} used${estimate.quota ? ` of ${formatBytes(estimate.quota)}` : ""}`
            : "unknown"}
        </dd>
        <dt>Last error</dt>
        <dd>{state.error ?? "—"}</dd>
      </dl>
      <div className="detail-actions">
        <button type="button" className="button-primary" onClick={() => void sync.refresh()} disabled={state.refreshing}>
          Refresh now
        </button>
        <button type="button" className="button-danger" onClick={() => void clearAndRefetch()}>
          Clear cache &amp; refetch
        </button>
      </div>
    </section>
  );
}
