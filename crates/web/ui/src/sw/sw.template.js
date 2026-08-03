// Classic-script service worker, templated at build time by the inline
// Vite plugin (__PRECACHE__ / __VERSION__). Deliberately boring:
// precache the hashed shell, cache-first assets, network-first
// navigations, and NEVER touch /api/* — that keeps it structurally
// immune to Safari's Range/Cache-API traps. Stateless on purpose: iOS
// can drop the worker context at any time.

/* eslint-disable no-restricted-globals */

const VERSION = "__VERSION__";
const CACHE = `rawr-${VERSION}`;
const PRECACHE = __PRECACHE__;

self.addEventListener("install", (event) => {
  // No skipWaiting(): swapping hashed assets under a live page 404s its
  // lazy chunks. The page opts in via the update toast instead.
  event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(PRECACHE)));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("message", (event) => {
  if (event.data && event.data.type === "SKIP_WAITING") self.skipWaiting();
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  if (url.pathname.startsWith("/api/")) return;

  if (request.mode === "navigate") {
    // Network-first with a short timeout; offline cold starts get the
    // cached shell and the app renders from IndexedDB.
    event.respondWith(
      fetch(request, { signal: AbortSignal.timeout(3000) }).catch(() =>
        caches.match("/").then((cached) => cached ?? Response.error()),
      ),
    );
    return;
  }

  if (url.pathname.startsWith("/assets/")) {
    // Content-hashed: cache-first, backfill on miss.
    event.respondWith(
      caches.match(request).then(
        (cached) =>
          cached ??
          fetch(request).then((response) => {
            if (response.ok) {
              const copy = response.clone();
              void caches.open(CACHE).then((cache) => cache.put(request, copy));
            }
            return response;
          }),
      ),
    );
  }
});
