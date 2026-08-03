import { sync } from "../data/instance";
import { formatAge } from "../format";
import { useSyncState } from "../hooks/useSync";

const DAY_S = 86_400;

/** Data age at a glance: quiet when fresh, amber past a day, red past a
 * week. Tapping it is a manual refresh. */
export function SyncChip() {
  const state = useSyncState();
  if (state.status !== "ready") return null;
  const age = state.fetchedAt ? Date.now() / 1000 - state.fetchedAt : 0;
  const tone = age > 7 * DAY_S ? "bad" : age > DAY_S ? "warn" : "ok";
  return (
    <button
      type="button"
      className={`sync-chip is-${tone}`}
      onClick={() => void sync.refresh()}
      disabled={state.refreshing}
      title="Refresh the library"
    >
      {state.refreshing ? "syncing…" : state.fetchedAt ? `synced ${formatAge(state.fetchedAt)}` : "not synced"}
    </button>
  );
}
