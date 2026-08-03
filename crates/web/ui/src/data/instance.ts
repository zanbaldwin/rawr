// The app's one sync-engine instance, wired with real dependencies.
// Everything testable lives behind createSync; this file is glue.

import { createCache } from "./cache";
import { createSync } from "./sync";

export const sync = createSync({
  fetchImpl: (input, init) => fetch(input, init),
  cache: createCache(),
  now: () => Date.now(),
});
