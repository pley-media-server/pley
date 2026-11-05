// Builds a TV app from its entry: one script and one stylesheet for the
// TV's old engine, the fonts, and the page that loads them. Everything the
// app needs ships inside the package: no CDN, no remote font, so it works
// on a home network without the internet. NOTICES.txt beside it carries the
// licenses of the packages in it (notices.ts).

import { build } from "esbuild";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { commit } from "../../../scripts/commit.mjs";
import { LICENSE_FILES, noticesText, packageOf, TEXTS } from "./notices.ts";

const require = createRequire(import.meta.url);

/**
 * Ahead of everything in the script: esbuild's helpers for object spread
 * read Object.getOwnPropertyDescriptors once, when the script starts, which
 * is before the app's own polyfills (src/polyfills.ts) run, and Chromium 53
 * (webOS 4) has no such function. Strict, as the bundle is.
 */
const PRELUDE = [
  '"use strict";',
  "if (!Object.getOwnPropertyDescriptors) Object.getOwnPropertyDescriptors = function (o) { var d = {}; Reflect.ownKeys(o).forEach(function (k) { d[k] = Object.getOwnPropertyDescriptor(o, k); }); return d; };",
].join("\n");

/**
 * The language catalogs (`i18n/locales/<language>/<area>.json`) as strings
 * for JSON.parse rather than as object literals: an engine reads a JSON
 * string much faster than the same object written as code, and they are a
 * fifth of the script.
 * @type {import("esbuild").Plugin}
 */
const catalogsAsJson = {
  name: "catalogs-as-json",
  setup(b) {
    b.onLoad({ filter: /[\\/]i18n[\\/]locales[\\/][^\\/]+[\\/][^\\/]+\.json$/ }, (args) => {
      const json = JSON.stringify(JSON.parse(readFileSync(args.path, "utf8")));
      return { contents: `export default JSON.parse(${JSON.stringify(json)});`, loader: "js" };
    });
  },
};

/**
 * @param {{
 *   app: string,
 *   entry: string,
 *   outdir: string,
 *   target: string,
 *   version: string,
 *   head?: string,
 *   files?: { from: string, to: string }[],
 *   minify?: boolean,
 *   tsconfig?: string,
 * }} o
 */
export async function buildApp(o) {
  rmSync(o.outdir, { recursive: true, force: true });
  mkdirSync(o.outdir, { recursive: true });
  const result = await build({
    entryPoints: { app: o.entry },
    bundle: true,
    outdir: o.outdir,
    format: "iife",
    target: o.target,
    minify: o.minify !== false,
    tsconfig: o.tsconfig,
    sourcemap: false,
    legalComments: "none",
    jsx: "automatic",
    jsxImportSource: "preact",
    // The fonts are copied next to the stylesheet as they are.
    external: ["*.woff2"],
    define: { __PLEY_VERSION__: JSON.stringify(o.version), __PLEY_COMMIT__: JSON.stringify(commit()) },
    banner: { js: PRELUDE },
    plugins: [catalogsAsJson],
    logLevel: "warning",
    metafile: true,
  });
  const fonts = join(dirname(require.resolve("@fontsource/roboto/package.json")), "files");
  mkdirSync(join(o.outdir, "fonts"), { recursive: true });
  for (const weight of [400, 500, 700]) {
    const name = `roboto-latin-${weight}-normal.woff2`;
    copyFileSync(join(fonts, name), join(o.outdir, "fonts", name));
  }
  for (const f of o.files ?? []) {
    mkdirSync(dirname(join(o.outdir, f.to)), { recursive: true });
    copyFileSync(f.from, join(o.outdir, f.to));
  }
  writeFileSync(join(o.outdir, "NOTICES.txt"), noticesText(o.app, shipped(Object.keys(result.metafile.inputs), dirname(fonts))));
  writeFileSync(
    join(o.outdir, "index.html"),
    [
      "<!DOCTYPE html>",
      '<html lang="en">',
      "<head>",
      '<meta charset="utf-8">',
      '<meta name="viewport" content="width=1920, height=1080, initial-scale=1">',
      "<title>Pley</title>",
      '<link rel="stylesheet" href="app.css">',
      o.head ?? "",
      "</head>",
      "<body>",
      '<div id="app"></div>',
      '<script src="app.js"></script>',
      "</body>",
      "</html>",
      "",
    ]
      .filter((l) => l !== "")
      .join("\n") + "\n",
  );
  const bytes = Object.values(result.metafile.outputs).reduce((n, out) => n + out.bytes, 0);
  return { bytes };
}

/**
 * The packages of others' work in the app: those the bundle's `inputs`
 * come from, and the font, each with its license's text.
 *
 * @param {string[]} inputs
 * @param {string} fontPackage
 */
function shipped(inputs, fontPackage) {
  /** @type {Map<string, string>} */
  const dirs = new Map([["@fontsource/roboto", fontPackage]]);
  for (const input of inputs) {
    const id = packageOf(input);
    if (!id || dirs.has(id)) continue;
    const path = resolve(input).replace(/\\/g, "/");
    dirs.set(id, path.slice(0, path.lastIndexOf(`/node_modules/${id}/`) + `/node_modules/${id}`.length));
  }
  return [...dirs].map(([id, dir]) => {
    const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    const file = LICENSE_FILES.filter((f) => existsSync(join(dir, f)))[0];
    return { id, version: pkg.version, homepage: typeof pkg.homepage === "string" ? pkg.homepage : null, text: file ? readFileSync(join(dir, file), "utf8") : (TEXTS[id] ?? "") };
  });
}

/** The app's version, from its package.json. */
export function versionOf(packageJson) {
  return JSON.parse(readFileSync(packageJson, "utf8")).version;
}
