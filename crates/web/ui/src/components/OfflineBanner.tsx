import { formatAge } from "../format";
import { useSyncState } from "../hooks/useSync";

/** Slim, never modal. Shown only when we have data but can't reach the
 * library server. */
export function OfflineBanner() {
  const state = useSyncState();
  if (state.status !== "ready" || state.online || state.refreshing) return null;
  const age = state.fetchedAt ? ` — showing data from ${formatAge(state.fetchedAt)}` : "";
  return <div className="offline-banner">Can’t reach the library{age}</div>;
}
