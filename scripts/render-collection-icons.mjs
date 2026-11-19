#!/usr/bin/env bun
// Renders the icons a collection's poster can carry (src/collection_art.rs in
// the server) from Lucide's, through a headless Chrome or Edge, into
// apps/server/assets/collection-icons/<name>.png: white strokes on a
// transparent square of 256 px, which the server draws over the color
// chosen. Run it again when the list changes; the server's test checks that
// every icon it offers has its picture.
//
//   bun scripts/render-collection-icons.mjs

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { renderer } from "./svg-render.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "apps/server/assets/collection-icons");
const lucide = join(root, "node_modules/lucide");

// The server's list (ICONS in apps/server/src/collection_art.rs), in its order.
const source = readFileSync(join(root, "apps/server/src/collection_art.rs"), "utf8");
const list = source.match(/pub const ICONS: &\[&str\] = &\[([\s\S]*?)\];/);
if (!list) throw new Error("no ICONS in collection_art.rs");
const names = [...list[1].matchAll(/"([a-z0-9-]+)"/g)].map((m) => m[1]);

const SIZE = 256;

// Names Lucide keeps only as another icon's alias: their files there.
const ALIASES = { music: "eighth-notes", fingerprint: "fingerprint-pattern" };

/** Lucide's icon `name` as SVG elements: its module exports [tag, attributes] pairs. */
async function elements(name) {
  const icon = (await import(join(lucide, `dist/esm/icons/${ALIASES[name] ?? name}.mjs`))).default;
  return icon
    .map(([tag, attrs]) => `<${tag} ${Object.entries(attrs).map(([k, v]) => `${k}="${v}"`).join(" ")}/>`)
    .join("");
}

mkdirSync(out, { recursive: true });
const draw = await renderer();
try {
  for (const name of names) {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">${await elements(name)}</svg>`;
    writeFileSync(join(out, `${name}.png`), await draw.png(svg, SIZE));
  }
} finally {
  draw.close();
}
const license = readFileSync(join(lucide, "LICENSE"), "utf8");
writeFileSync(join(out, "LICENSE.txt"), `These pictures are drawn from Lucide's icons (https://lucide.dev).\n\n${license}`);
console.log(`${names.length} icons in ${out}`);
