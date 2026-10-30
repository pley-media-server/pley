#!/usr/bin/env bun
// Renders the pictures pley's Kodi screens are drawn with
// (apps/kodi/plugin.video.pley/resources/skins/default/media), in the
// Android TV app's look: its Lucide icons, read
// from the Android app's own set (PleyIcons.kt), its buttons, tile masks and
// focus ring, the backdrop's gradients and the artwork placeholders. Sizes
// are the Android TV app's dp doubled, as Kodi's 1080-line screens are
// twice its 540.
//
// White pictures are tinted by Kodi (colordiffuse); masks are drawn as
// `diffuse` over artwork, which keeps their opaque parts.
//
//   bun scripts/render-kodi-skin.mjs

import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { renderer } from "./svg-render.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "apps/kodi/plugin.video.pley/resources/skins/default/media");

// pley's colours (apps/android/core/designsystem/.../PleyColors.kt).
const BG = "#0b0d10";
const FG = "#e9ebef";
const CONTROL = "#6c7584";
const OUTLINE = "#3a4250";
const ACCENT = "#38bdf8";
const ACCENT_BRIGHT = "#7dd3fc";
const ON_ACCENT = "#03131f";
const SURFACE2 = "#191d24";

/** Lucide's icons as the Android app has them: name → { filled, paths }. */
function lucide() {
  const source = readFileSync(join(root, "apps/android/core/designsystem/src/main/kotlin/app/pley/android/designsystem/icon/PleyIcons.kt"), "utf8");
  const icons = {};
  for (const m of source.matchAll(/PleyIcon\(\s*"([^"]+)",\s*(true|false),([\s\S]*?)\n\s*\)/g)) {
    icons[m[1]] = { filled: m[2] === "true", paths: [...m[3].matchAll(/"([^"]*)"/g)].map((p) => p[1]) };
  }
  return icons;
}

const ICONS = lucide();
// Lucide's icons the Kodi screens draw that the Android app's set doesn't
// have (yet): Live TV's `tv-minimal`, the web's.
const KODI_ONLY = {
  "tv-minimal": { filled: false, paths: ["M7 21h10", "M4 3h16a2 2 0 0 1 2 2v10a2 2 0 0 1 -2 2h-16a2 2 0 0 1 -2 -2v-10a2 2 0 0 1 2 -2z"] },
};
for (const [name, found] of Object.entries(KODI_ONLY)) ICONS[name] ??= found;

/** Icon [name], [size] px, in [color], with its top left at (x, y), as SVG elements. */
function icon(name, size, color, x = 0, y = 0) {
  const found = ICONS[name];
  if (!found) throw new Error(`no Lucide icon ${name} in PleyIcons.kt`);
  const fill = found.filled ? color : "none";
  const paths = found.paths.map((d) => `<path d="${d}"/>`).join("");
  return `<g transform="translate(${x} ${y}) scale(${size / 24})" fill="${fill}" stroke="${color}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${paths}</g>`;
}

const svg = (width, height, body) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}">${body}</svg>`;

/** A rounded rectangle [width] × [height] of radius [r], filled, with an edge [stroke] px wide inside it. */
function box(width, height, r, fill, edge, stroke = 0) {
  const inset = stroke / 2;
  const body = `<rect width="${width}" height="${height}" rx="${r}" fill="${fill}"/>`;
  if (!edge || !stroke) return body;
  return body + `<rect x="${inset}" y="${inset}" width="${width - stroke}" height="${height - stroke}" rx="${r - inset}" fill="none" stroke="${edge}" stroke-width="${stroke}"/>`;
}

/** CSS's hsl() as #rrggbb. */
function hsl(h, s, l) {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const hp = (h % 360) / 60;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  const [r, g, b] = hp < 1 ? [c, x, 0] : hp < 2 ? [x, c, 0] : hp < 3 ? [0, c, x] : hp < 4 ? [0, x, c] : hp < 5 ? [x, 0, c] : [c, 0, x];
  const m = l - c / 2;
  return "#" + [r, g, b].map((v) => Math.round((v + m) * 255).toString(16).padStart(2, "0")).join("");
}

const pictures = [];
const add = (name, width, height, body) => pictures.push({ name, width, height, svg: svg(width, height, body) });

// Icons, white, as big as the screens draw them: 40 px (20 dp) in the rail,
// buttons and rows; the star of a rating 28; See all's arrow 48.
for (const name of [
  "search", "house", "clapperboard", "tv", "music", "image", "folder", "list-music", "layers", "settings",
  "play", "refresh-cw", "arrow-up-narrow-wide", "arrow-down-wide-narrow", "sliders-horizontal", "chevron-down",
  "check", "circle-check", "server", "log-out", "lock", "tv-minimal", "chevron-right",
]) {
  add(`icon-${name}.png`, 40, 40, icon(name, 40, "#ffffff"));
}
add("icon-star.png", 28, 28, icon("star", 28, "#ffffff"));
add("icon-arrow-right.png", 48, 48, icon("arrow-right", 48, "#ffffff"));

// The round buttons (TvRoundButton): 80 square, corners 12, at rest bg at
// 80 % with the control edge and the icon in fg, focused the accent with
// the icon in on-accent.
for (const name of ["rotate-ccw", "circle-check", "eye-off", "bookmark-plus", "bookmark-check", "list-plus", "film", "ellipsis", "info", "shuffle"]) {
  add(`button-${name}.png`, 80, 80, box(80, 80, 12, "rgba(11,13,16,0.8)", CONTROL, 2) + icon(name, 40, FG, 20, 20));
  add(`button-${name}-focus.png`, 80, 80, box(80, 80, 12, ACCENT) + icon(name, 40, ON_ACCENT, 20, 20));
}

// Pills (TvPillButton), cut as nine-slice pictures: at rest fg at 12 % with
// the control edge, focused the accent. With an icon, it is in the left
// 80 px (40 of room, then the icon), which Kodi never stretches; the label
// starts after it.
add("pill.png", 48, 80, box(48, 80, 12, "rgba(233,235,239,0.12)", CONTROL, 2));
add("pill-focus.png", 48, 80, box(48, 80, 12, ACCENT));
for (const name of ["play", "refresh-cw", "arrow-up-down"]) {
  add(`pill-${name}.png`, 160, 80, box(160, 80, 12, "rgba(233,235,239,0.12)", CONTROL, 2) + icon(name, 40, FG, 40, 20));
  add(`pill-${name}-focus.png`, 160, 80, box(160, 80, 12, ACCENT) + icon(name, 40, ON_ACCENT, 40, 20));
}
// Chips (the library's sort and filters): surface3 at rest, no edge.
add("chip.png", 48, 80, box(48, 80, 12, "#222731"));
for (const name of ["arrow-up-narrow-wide", "arrow-down-wide-narrow", "sliders-horizontal"]) {
  add(`chip-${name}.png`, 160, 80, box(160, 80, 12, "#222731") + icon(name, 40, FG, 32, 20));
  add(`chip-${name}-focus.png`, 160, 80, box(160, 80, 12, ACCENT) + icon(name, 40, ON_ACCENT, 32, 20));
}
// A chip that is on (TvChip's selected) leads with a check, in the accent at rest.
add("chip-check.png", 160, 80, box(160, 80, 12, "#222731") + icon("check", 40, ACCENT, 32, 20));
add("chip-check-focus.png", 160, 80, box(160, 80, 12, ACCENT) + icon("check", 40, ON_ACCENT, 32, 20));

// Plain shapes, white, to tint: corners of 4 (badges), 8 (tiles), 12
// (controls); circles.
add("round4.png", 16, 16, box(16, 16, 4, "#ffffff"));
add("round8.png", 32, 32, box(32, 32, 8, "#ffffff"));
add("round12.png", 48, 48, box(48, 48, 12, "#ffffff"));
add("circle.png", 128, 128, `<circle cx="64" cy="64" r="64" fill="#ffffff"/>`);

// Tile masks, exactly each tile's size, corners 8 (4 dp).
for (const [name, width, height] of [
  ["poster", 216, 324],
  ["landscape", 408, 230],
  ["episode", 368, 208],
  ["grid", 192, 288],
  ["square", 264, 264],
]) {
  add(`mask-${name}.png`, width, height, box(width, height, 8, "#ffffff"));
}
add("mask-circle.png", 192, 192, `<circle cx="96" cy="96" r="96" fill="#ffffff"/>`);

// The focus ring (design spec §1.4): a 6 px accent-bright stroke whose
// middle runs 9 px outside the tile, radii 14 inside and 20 outside;
// nine-slice, drawn 12 px out from the tile all round. People's is a circle.
add("ring.png", 64, 64, `<rect x="3" y="3" width="58" height="58" rx="17" fill="none" stroke="${ACCENT_BRIGHT}" stroke-width="6"/>`);
add("ring-circle.png", 216, 216, `<circle cx="108" cy="108" r="105" fill="none" stroke="${ACCENT_BRIGHT}" stroke-width="6"/>`);

// What sits on art: the watched tick (bg at 80 %, an accent check), the
// line along a tile's inner edge (white at 6 %) and quality badges' edge.
add("tick.png", 40, 40, box(40, 40, 4, "rgba(11,13,16,0.8)") + `<path d="M28 14 17 25l-5-5" fill="none" stroke="${ACCENT}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>`);
add("hairline.png", 32, 32, box(32, 32, 8, "none", "rgba(255,255,255,0.06)", 2));
add("badge.png", 16, 16, box(16, 16, 4, "none", OUTLINE, 2));

// The search field: surface2, corners 8, a control edge at rest, an
// accent-bright one when focused.
add("field.png", 32, 32, box(32, 32, 8, SURFACE2, CONTROL, 2));
add("field-focus.png", 32, 32, box(32, 32, 8, SURFACE2, ACCENT_BRIGHT, 4));

// Gradients, white with alpha, tinted bg by Kodi: the backdrop's scrim from
// the left (bg at 100 % at 0, 92 % at 600 px, 85 % at 1120, 0 at 1600),
// fades from clear to solid downwards and rightwards, and the open rail's
// shadow.
const stops = (pairs) => pairs.map(([at, alpha]) => `<stop offset="${at}" stop-color="#ffffff" stop-opacity="${alpha}"/>`).join("");
add("scrim-left.png", 800, 4, `<defs><linearGradient id="g">${stops([[0, 1], [0.375, 0.92], [0.7, 0.85], [1, 0]])}</linearGradient></defs><rect width="800" height="4" fill="url(#g)"/>`);
add("fade-down.png", 4, 256, `<defs><linearGradient id="g" x2="0" y2="1">${stops([[0, 0], [1, 1]])}</linearGradient></defs><rect width="4" height="256" fill="url(#g)"/>`);
add("fade-left.png", 64, 4, `<defs><linearGradient id="g">${stops([[0, 1], [1, 0]])}</linearGradient></defs><rect width="64" height="4" fill="url(#g)"/>`);
add("shadow.png", 48, 4, `<defs><linearGradient id="g">${stops([[0, 0.45], [1, 0]])}</linearGradient></defs><rect width="48" height="4" fill="url(#g)"/>`);

// The sign-in screen's: the logo (its tile and P, 80 px as the Android app
// draws it) and the brand's glow from the lower right (brandGlow: brand-deep
// at 18 % to nothing, 60 % of the width out), stretched over the screen.
pictures.push({ name: "logo.png", width: 80, height: 80, svg: readFileSync(join(root, "apps/web/public/favicon.svg"), "utf8") });
add(
  "glow.png",
  480,
  270,
  `<defs><radialGradient id="g" cx="480" cy="270" r="288" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#0284c7" stop-opacity="0.18"/><stop offset="1" stop-color="#0284c7" stop-opacity="0"/></radialGradient></defs><rect width="480" height="270" fill="url(#g)"/>`,
);

// Placeholders, as the web's placeholderGradient: 135° from hsl(h, 30 %,
// 22 %) to hsl(h + 40, 35 %, 12 %), h from a hash of the title. Kodi gets
// 24 of them, h every 15°, and the add-on picks the nearest.
for (let i = 0; i < 24; i++) {
  const h = i * 15;
  add(
    `ph-${String(i).padStart(2, "0")}.png`,
    96,
    96,
    `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${hsl(h, 0.3, 0.22)}"/><stop offset="1" stop-color="${hsl((h + 40) % 360, 0.35, 0.12)}"/></linearGradient></defs><rect width="96" height="96" fill="url(#g)"/>`,
  );
}

mkdirSync(out, { recursive: true });
// Pictures this script made before and no longer makes go; others stay (pley-white.png).
const made = new Set(pictures.map((p) => p.name));
for (const name of readdirSync(out)) {
  if (!made.has(name) && /^(icon|button|pill|chip|round|circle|mask|ring|tick|hairline|badge|field|scrim|fade|shadow|ph)-?/.test(name) && name !== "pley-white.png") {
    rmSync(join(out, name));
  }
}
const r = await renderer();
try {
  for (const p of pictures) writeFileSync(join(out, p.name), await r.png(p.svg, p.width, p.height));
  console.log(`rendered ${pictures.length} pictures into ${out}`);
} finally {
  r.close();
}
