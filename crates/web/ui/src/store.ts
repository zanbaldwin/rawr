// The app's entire state-management "library": ~25 lines feeding
// React's useSyncExternalStore. Snapshots are immutable; `patch`
// spreads — the React Compiler depends on that.

export interface Store<T> {
  get: () => T;
  set: (next: T) => void;
  patch: (partial: Partial<T>) => void;
  subscribe: (listener: () => void) => () => void;
}

export function createStore<T>(initial: T): Store<T> {
  let value = initial;
  const listeners = new Set<() => void>();
  const set = (next: T): void => {
    if (Object.is(value, next)) return;
    value = next;
    for (const listener of listeners) listener();
  };
  return {
    get: () => value,
    set,
    patch: (partial) => set({ ...value, ...partial }),
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
