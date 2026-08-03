import { useSyncExternalStore } from "react";
import type { Store } from "../store";

/** The sanctioned tear-free bridge from module-level stores into React
 * (the engine starts before createRoot and must survive StrictMode). */
export function useStore<T>(store: Store<T>): T {
  return useSyncExternalStore(store.subscribe, store.get);
}
