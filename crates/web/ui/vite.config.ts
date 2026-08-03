import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { defineConfig } from "vite";
import type { Plugin } from "vite";
import react from "@vitejs/plugin-react";

/** Emit sw.js from src/sw/sw.template.js with the precache list of this
 * build's hashed assets. Stable root name on purpose — a hashed worker
 * cannot claim scope "/". */
function emitServiceWorker(): Plugin {
  return {
    name: "rawr-emit-sw",
    apply: "build",
    generateBundle(_options, bundle) {
      const precache = ["/"];
      for (const file of Object.keys(bundle)) {
        if (/^assets\/.+\.(js|css|svg|woff2)$/.test(file)) precache.push(`/${file}`);
      }
      precache.sort();
      const version = createHash("sha256").update(precache.join("\n")).digest("hex").slice(0, 12);
      const template = readFileSync(join(import.meta.dirname, "src/sw/sw.template.js"), "utf8");
      const source = template.replaceAll("__VERSION__", version).replaceAll("__PRECACHE__", JSON.stringify(precache));
      this.emitFile({ type: "asset", fileName: "sw.js", source });
    },
  };
}

export default defineConfig({
  // CWD-independent so `npm --prefix crates/web/ui run build` works from
  // anywhere (rollup resolves relative inputs against the CWD, not root).
  root: import.meta.dirname,
  base: "/",
  plugins: [react(), emitServiceWorker()],
  build: {
    outDir: "dist",
    // Load-bearing: rust-embed bakes the whole tree into the release
    // binary, so stale hashed files must not survive a rebuild.
    emptyOutDir: true,
    // Read by crates/web/build.rs and vite-chunks (dist/.vite/manifest.json).
    manifest: true,
    sourcemap: false,
    rollupOptions: {
      output: {
        entryFileNames: "assets/[name].[hash].js",
        chunkFileNames: "assets/[name].[hash].js",
        assetFileNames: "assets/[name].[hash][extname]",
      },
    },
  },
  server: {
    proxy: {
      "/api": { target: "http://127.0.0.1:8080" },
    },
  },
});
