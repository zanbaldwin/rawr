import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  // CWD-independent so `npm --prefix crates/web/ui run build` works from
  // anywhere (rollup resolves relative inputs against the CWD, not root).
  root: import.meta.dirname,
  base: "/",
  plugins: [react()],
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
