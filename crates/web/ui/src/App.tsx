import { useEffect, useState } from "react";
import type { ApiMeta } from "./types/generated/ApiMeta";

/** Placeholder shell: proves the embed + dev proxy end to end. The real
 * screens land with the data layer. */
export function App() {
  const [meta, setMeta] = useState<ApiMeta | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch("/api/v1/meta")
      .then((res) => res.json() as Promise<ApiMeta>)
      .then(setMeta)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  return (
    <main className="placeholder">
      <h1>🦖 rawr</h1>
      {meta && (
        <p>
          Library “{meta.target}”: {meta.works.toLocaleString()} works, {meta.versions.toLocaleString()} versions.
        </p>
      )}
      {error && <p>Can’t reach the library server: {error}</p>}
      {!meta && !error && <p>Reaching the library…</p>}
    </main>
  );
}
