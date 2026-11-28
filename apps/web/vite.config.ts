import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { extname, join, resolve } from "node:path";
import { brotliCompressSync, constants as zlib, gzipSync } from "node:zlib";
import { commit } from "../../scripts/commit.mjs";

/** The web app's version: the server's, which it is released with (scripts/version.mjs). */
const VERSION = (JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as { version: string }).version;

const COMPRESSIBLE = new Set([".js", ".mjs", ".css", ".html", ".svg", ".json", ".webmanifest", ".txt"]);

/**
 * Writes `.br` and `.gz` siblings for every compressible asset in the build output.
 * The Rust server serves these precompressed variants based on `Accept-Encoding`,
 * so nothing has to be compressed at request time.
 */
function precompress(): Plugin {
  let outDir = "dist";
  return {
    name: "pley-precompress",
    apply: "build",
    configResolved(config) {
      outDir = resolve(config.root, config.build.outDir);
    },
    closeBundle() {
      const walk = (dir: string) => {
        for (const name of readdirSync(dir)) {
          const path = join(dir, name);
          const stat = statSync(path);
          if (stat.isDirectory()) {
            walk(path);
            continue;
          }
          if (!COMPRESSIBLE.has(extname(name)) || stat.size <= 1024) continue;
          const data = readFileSync(path);
          writeFileSync(
            `${path}.br`,
            brotliCompressSync(data, {
              params: {
                [zlib.BROTLI_PARAM_QUALITY]: 11,
                [zlib.BROTLI_PARAM_SIZE_HINT]: data.length,
              },
            }),
          );
          writeFileSync(`${path}.gz`, gzipSync(data, { level: 9 }));
        }
      };
      walk(outDir);
    },
  };
}

export default defineConfig({
  plugins: [react(), tailwindcss(), precompress()],
  // Shown in Account and the dashboard (src/lib/build.ts).
  define: {
    __PLEY_VERSION__: JSON.stringify(VERSION),
    __PLEY_COMMIT__: JSON.stringify(commit()),
  },
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:7539",
        changeOrigin: false,
        // Keep SSE (`/api/events`) streaming through the proxy.
        configure(proxy) {
          proxy.on("proxyRes", (proxyRes) => {
            if (proxyRes.headers["content-type"]?.includes("text/event-stream")) {
              proxyRes.headers["cache-control"] = "no-cache";
              proxyRes.headers["x-accel-buffering"] = "no";
            }
          });
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    // No styles, but index.css itself, which a test reads as text (the
    // themes' colors, src/theme/palettes.test.ts).
    css: { include: [/\/src\/index\.css/] },
    // Tests that type a password take about a second; on a machine busy
    // building they took over vitest's 5 s and failed for nothing.
    testTimeout: 15_000,
    restoreMocks: true,
    unstubGlobals: true,
    coverage: {
      // Every source file counts, tested or not.
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/**/*.test.{ts,tsx}", "src/test/**", "src/main.tsx"],
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
    chunkSizeWarningLimit: 800,
    rollupOptions: {
      output: {
        // Stable vendor chunks cache well across app releases.
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (/[\\/]node_modules[\\/](react|react-dom|react-router|scheduler|cookie|set-cookie-parser)[\\/]/.test(id)) return "react";
          if (/[\\/]node_modules[\\/]@tanstack[\\/]/.test(id)) return "query";
          if (/[\\/]node_modules[\\/]lucide-react[\\/]/.test(id)) return "icons";
          return undefined;
        },
      },
    },
  },
});
