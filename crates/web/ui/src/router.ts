// Hand-rolled hash router. Filter state lives in the hash query string so
// back/forward, reload, and relaunch from the Home Screen all preserve
// the view; the server only ever sees `/`.

import { createStore } from "./store";
import type { Store } from "./store";

export type Route =
  | { name: "list" }
  | { name: "work"; id: number }
  | { name: "upload" }
  | { name: "stats" }
  | { name: "about" }
  | { name: "notfound" };

export interface RouteState {
  route: Route;
  params: URLSearchParams;
}

/** Never throws on junk — a hand-typed URL must not break the app. */
export function parseHash(hash: string): RouteState {
  const raw = hash.startsWith("#") ? hash.slice(1) : hash;
  const [pathPart = "", queryPart = ""] = raw.split("?", 2);
  const params = new URLSearchParams(queryPart);
  const segments = pathPart.split("/").filter((s) => s !== "");
  if (segments.length === 0) return { route: { name: "list" }, params };
  switch (segments[0]) {
    case "works": {
      const id = Number(segments[1]);
      if (segments.length === 2 && Number.isInteger(id) && id > 0) {
        return { route: { name: "work", id }, params };
      }
      return { route: { name: "notfound" }, params };
    }
    case "upload":
      return segments.length === 1 ? { route: { name: "upload" }, params } : { route: { name: "notfound" }, params };
    case "stats":
      return segments.length === 1 ? { route: { name: "stats" }, params } : { route: { name: "notfound" }, params };
    case "about":
      return segments.length === 1 ? { route: { name: "about" }, params } : { route: { name: "notfound" }, params };
    default:
      return { route: { name: "notfound" }, params };
  }
}

export function routeToHash(route: Route, params?: URLSearchParams): string {
  const query = params && params.size > 0 ? `?${params.toString()}` : "";
  switch (route.name) {
    case "list":
      return `#/${query}`;
    case "work":
      return `#/works/${route.id}`;
    case "upload":
      return "#/upload";
    case "stats":
      return "#/stats";
    case "about":
      return "#/about";
    case "notfound":
      return "#/404";
  }
}

export const routeStore: Store<RouteState> = createStore<RouteState>(
  typeof window === "undefined" ? { route: { name: "list" }, params: new URLSearchParams() } : parseHash(window.location.hash),
);

export function navigate(hash: string, opts: { replace?: boolean } = {}): void {
  if (opts.replace) {
    history.replaceState(null, "", hash);
    // replaceState fires no hashchange — update the store by hand.
    routeStore.set(parseHash(hash));
  } else {
    // Assigning location.hash fires hashchange, which updates the store.
    window.location.hash = hash;
  }
}

/** Call once from main. */
export function attachRouter(): void {
  window.addEventListener("hashchange", () => {
    routeStore.set(parseHash(window.location.hash));
  });
}
