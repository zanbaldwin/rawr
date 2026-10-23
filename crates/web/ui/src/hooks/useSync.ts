import { reads, sync } from "../data/instance";
import type { ReadsState } from "../data/reads";
import type { SyncState } from "../data/sync";
import type { Library } from "../types";
import { useStore } from "./useStore";

export function useSyncState(): SyncState {
  return useStore(sync.syncStore);
}

export function useLibrary(): Library | null {
  return useStore(sync.libraryStore);
}

export function useReads(): ReadsState {
  return useStore(reads.readsStore);
}
