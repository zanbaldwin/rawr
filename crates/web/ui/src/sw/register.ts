// THE swappable PWA seam: nothing else in the app knows a service worker
// exists. Switching to vite-plugin-pwa later means rewriting this file
// and the emit plugin, nothing more.

import { createStore } from "../store";
import type { Sync } from "../data/sync";

export const updateStore = createStore<{ waiting: ServiceWorker | null }>({ waiting: null });

let reloading = false;

export function attachPwa(sync: Sync): void {
  if (!import.meta.env.PROD || !("serviceWorker" in navigator)) return;

  void navigator.serviceWorker
    .register("/sw.js", { scope: "/", updateViaCache: "none" })
    .then((registration) => {
      // An update that landed on a previous visit.
      if (registration.waiting) updateStore.set({ waiting: registration.waiting });
      registration.addEventListener("updatefound", () => {
        const installing = registration.installing;
        if (!installing) return;
        installing.addEventListener("statechange", () => {
          // installed + an active controller = a NEW version waiting.
          if (installing.state === "installed" && navigator.serviceWorker.controller) {
            updateStore.set({ waiting: registration.waiting });
          }
        });
      });
      // Browsers clamp the script's max-age and check daily at most;
      // nudge on foreground (throttled by the browser itself).
      document.addEventListener("visibilitychange", () => {
        if (document.visibilityState === "visible") void registration.update().catch(() => undefined);
      });
    })
    .catch(() => undefined);

  // Exactly one reload when the new worker takes over.
  navigator.serviceWorker.addEventListener("controllerchange", () => {
    if (reloading) return;
    reloading = true;
    location.reload();
  });

  attachPersistence(sync);
}

export function applyUpdate(): void {
  updateStore.get().waiting?.postMessage({ type: "SKIP_WAITING" });
  updateStore.set({ waiting: null });
}

/** Ask for eviction protection once the first sync lands. WebKit's
 * documented auto-grant heuristic is "opened as a Home Screen Web App" —
 * no prompt either way. */
function attachPersistence(sync: Sync): void {
  const unsubscribe = sync.syncStore.subscribe(() => {
    const state = sync.syncStore.get();
    if (state.status === "ready" && state.online) {
      unsubscribe();
      void navigator.storage
        ?.persisted?.()
        .then((persisted) => (persisted ? true : navigator.storage.persist?.()))
        .catch(() => undefined);
    }
  });
}
