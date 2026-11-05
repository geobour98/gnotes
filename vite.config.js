import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// Dev only: never answer "304 Not Modified". After a dev-server restart the
// webview would otherwise reuse cached component JS without the server
// compiling it, and the component's CSS then fails to load ("failed to load
// virtual css module"), leaving the app unstyled.
/** @type {import("vite").Plugin} */
const noDevCache = {
  name: "gnotes-no-dev-cache",
  apply: "serve",
  configureServer(server) {
    server.middlewares.use((req, res, next) => {
      // (typed loosely: this project has no @types/node)
      const headers = /** @type {any} */ (req).headers;
      delete headers["if-none-match"];
      delete headers["if-modified-since"];
      res.setHeader("Cache-Control", "no-store");
      next();
    });
  },
};

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [noDevCache, sveltekit()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
