#!/usr/bin/env bun
// Draws Pley's placeholder mark (packages/placeholder/placeholder.json), one
// for each glyph, for the apps that ship it as a file rather than drawing
// its paths:
//
//   apps/tizen-native/src/Pley.Tv.Tizen/shared/res/placeholder-mark-<glyph>.svg
//                                           the native Samsung app's, which NUI
//                                           draws at the size it shows
//   apps/kodi/plugin.video.pley/resources/skins/default/media/mark-<glyph>.png
//                                           Kodi's, 192 px, under a tile's picture
//   apps/roku/images/placeholder-<shape>-<glyph>.png
//                                           Roku's, a poster's failed picture: the
//                                           mark in the middle of a picture the
//                                           shape of the tile (poster, wide, square),
//                                           so that it scales without stretching
//
// Pictures it made before and no longer makes go. Chrome or Edge draws the
// PNGs (svg-render.mjs).
//
//   bun scripts/render-placeholder-marks.mjs

import { readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { renderer } from "./svg-render.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const marks = JSON.parse(readFileSync(join(root, "packages/placeholder/placeholder.json"), "utf8"));
const glyphs = Object.keys(marks.glyphs);

/** The mark's paths with `glyph` cut out of the front poster, white at each poster's alpha. */
function paths(glyph) {
  const last = marks.posters.length - 1;
  return marks.posters
    .map((p, i) =>
      i === last
        ? `<path fill="#ffffff" fill-opacity="${p.alpha}" fill-rule="evenodd" d="${p.path}${marks.glyphs[glyph]}"/>`
        : `<path fill="#ffffff" fill-opacity="${p.alpha}" d="${p.path}"/>`,
    )
    .join("\n  ");
}

/** The mark `side` square at (x, y) in a picture `width` × `height`. */
function picture(glyph, width, height, x, y, side) {
  const scale = side / marks.grid;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}"><g transform="translate(${x} ${y}) scale(${scale})">${paths(glyph)}</g></svg>`;
}

/** Removes the files in `folder` that `pattern` matches and this run didn't make. */
function sweep(folder, pattern, made) {
  for (const name of readdirSync(folder)) {
    if (pattern.test(name) && !made.has(name)) rmSync(join(folder, name));
  }
}

// The native Samsung app: SVGs on the mark's own grid.
const samsung = join(root, "apps/tizen-native/src/Pley.Tv.Tizen/shared/res");
const samsungMade = new Set();
for (const glyph of glyphs) {
  const name = `placeholder-mark-${glyph}.svg`;
  writeFileSync(join(samsung, name), `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${marks.grid} ${marks.grid}">\n  ${paths(glyph)}\n</svg>\n`);
  samsungMade.add(name);
}
sweep(samsung, /^placeholder-mark-.*\.svg$/, samsungMade);

const draw = await renderer();
try {
  // Kodi: the mark alone, as large as a 1080-line tile needs; the skin places and sizes it.
  const kodi = join(root, "apps/kodi/plugin.video.pley/resources/skins/default/media");
  const kodiMade = new Set();
  for (const glyph of glyphs) {
    const name = `mark-${glyph}.png`;
    writeFileSync(join(kodi, name), await draw.png(picture(glyph, 192, 192, 0, 0, 192), 192, 192));
    kodiMade.add(name);
  }
  sweep(kodi, /^mark-.*\.png$/, kodiMade);

  // Roku: the shapes of components/items/Tile.brs, the mark at `share` of the shorter side.
  const roku = join(root, "apps/roku/images");
  const rokuMade = new Set();
  for (const [shape, width, height] of [
    ["poster", 240, 360],
    ["wide", 400, 225],
    ["square", 300, 300],
  ]) {
    const side = Math.min(width, height) * marks.share;
    for (const glyph of glyphs) {
      const name = `placeholder-${shape}-${glyph}.png`;
      writeFileSync(join(roku, name), await draw.png(picture(glyph, width, height, (width - side) / 2, (height - side) / 2, side), width, height));
      rokuMade.add(name);
    }
  }
  sweep(roku, /^placeholder-.*\.png$/, rokuMade);
} finally {
  draw.close();
}
console.log(`${glyphs.length} glyphs: ${glyphs.join(", ")}`);
