// The app's one sync-engine instance, wired with real dependencies.
// Everything testable lives behind createSync; this file is glue.

import { createCache } from "./cache";
import { createReads } from "./reads";
import { createSync } from "./sync";

const cache = createCache();

export const sync = createSync({
  fetchImpl: (input, init) => fetch(input, init),
  cache,
  now: () => Date.now(),
});

export const reads = createReads({
  fetchImpl: (input, init) => fetch(input, init),
  cache,
});
