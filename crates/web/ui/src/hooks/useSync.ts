import { sync } from "../data/instance";
import type { SyncState } from "../data/sync";
import type { Library } from "../types";
import { useStore } from "./useStore";

export function useSyncState(): SyncState {
  return useStore(sync.syncStore);
}

export function useLibrary(): Library | null {
  return useStore(sync.libraryStore);
}
