import { useEffect, useRef, useState } from "react";
import { sync } from "../data/instance";
import { formatAge, formatBytes, formatNumber } from "../format";
import { useLibrary, useSyncState } from "../hooks/useSync";
import type { ScanStatusDto } from "../types/generated/ScanStatusDto";

/** Sync state, storage, and the escape hatch for "it's showing stale
 * data": clear everything and refetch. */
export function AboutScreen() {
  const state = useSyncState();
  const library = useLibrary();
  const [estimate, setEstimate] = useState<StorageEstimate | null>(null);
  const [scan, setScan] = useState<ScanStatusDto | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    navigator.storage
      ?.estimate?.()
      .then(setEstimate)
      .catch(() => setEstimate(null));
    // Show the server's last scan (and adopt one already in flight).
    fetchScan("GET").then((status) => {
      if (alive.current && status) {
        setScan(status);
        if (status.running) void watch(status);
      }
    });
    return () => {
      alive.current = false;
    };
  }, []);

  const watch = async (initial: ScanStatusDto) => {
    let status = initial;
    while (status.running && alive.current) {
      await new Promise((resolve) => setTimeout(resolve, 1000));
      const polled = await fetchScan("GET");
      if (!polled) break;
      status = polled;
      if (alive.current) setScan(status);
    }
    // New works reach the list through a normal sync.
    if (status.changed > 0 && !status.running) void sync.refresh();
  };

  const rescan = async () => {
    setScanError(null);
    const status = await fetchScan("POST");
    if (!status) {
      setScanError("The scan couldn’t be started — is the server reachable?");
      return;
    }
    setScan(status);
    void watch(status);
  };

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
        <button
          type="button"
          className="button-secondary"
          onClick={() => void rescan()}
          disabled={!state.online || scan?.running === true}
          title="Walk the library storage for works imported from another machine"
        >
          {scan?.running ? "Scanning…" : "Rescan library"}
        </button>
        <button type="button" className="button-danger" onClick={() => void clearAndRefetch()}>
          Clear cache &amp; refetch
        </button>
      </div>
      {scan && (
        <p className="hint">
          {scan.running
            ? `Scanning ${formatNumber(scan.processed)} of ${formatNumber(scan.discovered)} files…`
            : scan.finished_at !== null
              ? `Last scan ${formatAge(scan.finished_at)}: ${formatNumber(scan.processed)} files, ` +
                `${formatNumber(scan.changed)} changed` +
                (scan.errors > 0 ? `, ${formatNumber(scan.errors)} failed` : "")
              : null}
        </p>
      )}
      {scanError && <p className="hint">{scanError}</p>}
    </section>
  );
}

async function fetchScan(method: "GET" | "POST"): Promise<ScanStatusDto | null> {
  try {
    const response = await fetch("/api/v1/scan", { method, cache: "no-store" });
    if (!response.ok) return null;
    return (await response.json()) as ScanStatusDto;
  } catch {
    return null;
  }
}
