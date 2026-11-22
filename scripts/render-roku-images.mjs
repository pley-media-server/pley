#!/usr/bin/env bun
// Renders the pictures Pley's Roku screens are drawn with
// (apps/roku/images/ui), in the Android TV app's look: its Lucide icons,
// read from the Android app's own set (PleyIcons.kt), and the shapes its
// controls are made of. Sizes are the Android TV app's dp doubled, as a
// Roku's 1080-line screen is twice its 540.
//
// Most pictures are white, for the screens to tint (a Poster's
// blendColor). Nine-patch pictures (.9.png) carry Roku's one-pixel border:
// black on the top and left edges marks what stretches.
//
//   bun scripts/render-roku-images.mjs

import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { renderer } from "./svg-render.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "apps/roku/images/ui");

// Pley's colors (apps/roku/components/shared/Ui.brs, Android's PleyColors.kt).
const FG = "#ebebeb";
const ACCENT = "#38bdf8";
const ACCENT_BRIGHT = "#7dd3fc";
const BG = "#16191e";

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
// The player's sleep timer, as the Android TV player has it (TvPlayerExtras.kt).
ICONS.moon ??= { filled: false, paths: ["M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"] };

/** Icon [name], [size] px, in [color], with its top left at (x, y), as SVG elements. */
function icon(name, size, color, x = 0, y = 0) {
  const found = ICONS[name];
  if (!found) throw new Error(`no Lucide icon ${name} in PleyIcons.kt`);
  const fill = found.filled ? color : "none";
  const paths = found.paths.map((d) => `<path d="${d}"/>`).join("");
  return `<g transform="translate(${x} ${y}) scale(${size / 24})" fill="${fill}" stroke="${color}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${paths}</g>`;
}

const svg = (width, height, body) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}">${body}</svg>`;

const pictures = [];
const add = (name, width, height, body) => pictures.push({ name, width, height, svg: svg(width, height, body) });

/**
 * A nine-patch picture: [body] drawn [width] × [height] inside Roku's
 * one-pixel border, whose top and left marks stretch the middle, leaving
 * [corner] px at each end as they are.
 */
function ninePatch(name, width, height, corner, body) {
  const marks =
    `<rect x="${1 + corner}" y="0" width="${width - 2 * corner}" height="1" fill="#000000" shape-rendering="crispEdges"/>` +
    `<rect x="0" y="${1 + corner}" width="1" height="${height - 2 * corner}" fill="#000000" shape-rendering="crispEdges"/>`;
  add(name, width + 2, height + 2, `<g transform="translate(1 1)">${body}</g>${marks}`);
}

/** A rounded rectangle [width] × [height] of radius [r], filled. */
const box = (width, height, r, fill) => `<rect width="${width}" height="${height}" rx="${r}" fill="${fill}"/>`;

// Icons, white, as big as the screens draw them: 40 px (20 dp) in buttons
// and rows, the player's previous and next 48, its play and pause 52.
const ICONS_40 = [
  "captions", "audio-lines", "gauge", "info", "moon", "list-video", "check", "play", "pause", "rotate-ccw",
  "circle-check", "bookmark-plus", "bookmark-check", "list-plus", "ellipsis", "film", "eye-off", "shuffle",
  "search", "house", "clapperboard", "tv", "music", "list-music", "layers", "settings", "compass", "radio",
  "star", "chevron-right", "chevron-left", "arrow-up-narrow-wide", "arrow-down-wide-narrow", "sliders-horizontal",
  "skip-forward", "skip-back", "x", "clock", "calendar-days", "user-round", "users", "image", "library-big",
  "tv-minimal", "arrow-right", "arrow-up-down", "globe", "history", "activity", "log-out", "server", "plus", "pencil", "square",
];
for (const name of ICONS_40) add(`icon-${name}-40.png`, 40, 40, icon(name, 40, "#ffffff"));
for (const name of ["skip-back", "skip-forward"]) add(`icon-${name}-48.png`, 48, 48, icon(name, 48, "#ffffff"));
for (const name of ["play", "pause"]) add(`icon-${name}-52.png`, 52, 52, icon(name, 52, "#ffffff"));

// Rounded boxes to tint: corners of 4 (badges), 8 (tiles), 12 (buttons and
// the player's focused controls), and the bar of a scrubber or a progress
// line (8 high, its ends round).
ninePatch("r4.9.png", 16, 16, 4, box(16, 16, 4, "#ffffff"));
ninePatch("r8.9.png", 32, 32, 8, box(32, 32, 8, "#ffffff"));
ninePatch("r12.9.png", 48, 48, 12, box(48, 48, 12, "#ffffff"));
ninePatch("bar.9.png", 16, 8, 4, box(16, 8, 4, "#ffffff"));

// The side panels and the focus in their lists: square, as menus are;
// buttons and the player's own controls keep their round corners.
ninePatch("panel.9.png", 80, 80, 32, `<rect width="80" height="80" fill="#ffffff"/>`);
ninePatch("square.9.png", 16, 16, 4, `<rect width="16" height="16" fill="#ffffff"/>`);

// The edge of the seek preview: 2 px, corners 8, white for fg at 20 %.
ninePatch("frame.9.png", 32, 32, 8, `<rect x="1" y="1" width="30" height="30" rx="7" fill="none" stroke="#ffffff" stroke-width="2"/>`);

// The scrubber's thumb: 20 px in fg; focused, 24 px with an accent-bright
// ring 4 px out, 4 px wide (36 across).
add("thumb.png", 20, 20, `<circle cx="10" cy="10" r="10" fill="#ffffff"/>`);
add("thumb-focus.png", 36, 36, `<circle cx="18" cy="18" r="12" fill="${FG}"/><circle cx="18" cy="18" r="16" fill="none" stroke="${ACCENT_BRIGHT}" stroke-width="4"/>`);

// Buffering: an 80 px ring, 4 px in fg, with a quarter in the accent, which
// the player turns.
add(
  "buffering.png",
  80,
  80,
  `<circle cx="40" cy="40" r="38" fill="none" stroke="${FG}" stroke-width="4"/>` +
    `<path d="M40 2A38 38 0 0 1 78 40" fill="none" stroke="${ACCENT}" stroke-width="4" stroke-linecap="round"/>`,
);

// The scrim behind the player's controls, stretched over the screen: black
// at 65 % at the top behind the title, 25 % through the middle, 70 % and
// then 90 % at the bottom, where the controls stand (Android TV's
// controlScrims).
const stops = (pairs, color = "#000000") => pairs.map(([at, alpha]) => `<stop offset="${at}" stop-color="${color}" stop-opacity="${alpha}"/>`).join("");
add(
  "scrim-player.png",
  4,
  256,
  `<defs><linearGradient id="g" x2="0" y2="1">${stops([[0, 0.65], [0.22, 0.25], [0.55, 0.25], [0.78, 0.7], [1, 0.9]])}</linearGradient></defs><rect width="4" height="256" fill="url(#g)"/>`,
);

// The title banner's band as each item begins: black at 65 % (the
// controls' scrim's top) to clear (Android TV's TitleBand).
add(
  "band-top.png",
  4,
  256,
  `<defs><linearGradient id="g" x2="0" y2="1">${stops([[0, 0.65], [1, 0]])}</linearGradient></defs><rect width="4" height="256" fill="url(#g)"/>`,
);

// The focus ring around a tile (Android TV's, the Kodi skin's): a 6 px
// accent-bright stroke whose middle runs 9 px outside the tile, drawn 12 px
// out from it all round; nine-patch, its corners left whole.
ninePatch("ring.9.png", 64, 64, 22, `<rect x="3" y="3" width="58" height="58" rx="17" fill="none" stroke="${ACCENT_BRIGHT}" stroke-width="6"/>`);
// The same ring around a person's circle, as wide as a row's (192 px and
// 12 px each side).
add("ring-round.png", 216, 216, `<circle cx="108" cy="108" r="105" fill="none" stroke="${ACCENT_BRIGHT}" stroke-width="6"/>`);

// The line along a tile's inner edge (white at 6 %), corners 8.
ninePatch("hairline.9.png", 32, 32, 10, `<rect x="1" y="1" width="30" height="30" rx="7" fill="none" stroke="rgba(255,255,255,0.06)" stroke-width="2"/>`);

// The watched tick: bg at 80 %, corners 4, an accent check.
add(
  "tick.png",
  40,
  40,
  box(40, 40, 4, "rgba(22,25,30,0.8)") + `<path d="M28 14 17 25l-5-5" fill="none" stroke="${ACCENT}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>`,
);

// The backdrop's scrims, in the app's background: from the left (100 % at
// 0, 92 % at 600 px, 85 % at 1120, nothing at 1600 of the screen), and from
// clear to solid downwards, stretched as each page needs.
add(
  "scrim-left.png",
  800,
  4,
  `<defs><linearGradient id="g">${stops([[0, 1], [0.375, 0.92], [0.7, 0.85], [1, 0]], BG)}</linearGradient></defs><rect width="800" height="4" fill="url(#g)"/>`,
);
add(
  "fade-down.png",
  4,
  256,
  `<defs><linearGradient id="g" x2="0" y2="1">${stops([[0, 0], [1, 1]], BG)}</linearGradient></defs><rect width="4" height="256" fill="url(#g)"/>`,
);

// A pill's edge (TvPillButton, TvRoundButton at rest): 2 px, corners 12,
// white for the control color.
ninePatch("edge.9.png", 48, 48, 12, `<rect x="1" y="1" width="46" height="46" rx="11" fill="none" stroke="#ffffff" stroke-width="2"/>`);

// A badge's edge on a page (the content rating, the quality): 2 px,
// corners 4, white for the outline's color.
ninePatch("edge4.9.png", 16, 16, 4, `<rect x="1" y="1" width="14" height="14" rx="3" fill="none" stroke="#ffffff" stroke-width="2"/>`);

// A circle to tint: the rail's avatar.
add("circle.png", 80, 80, `<circle cx="40" cy="40" r="40" fill="#ffffff"/>`);
// A settings switch's track (TvSwitch), tinted by the row.
add("switch.png", 80, 44, box(80, 44, 22, "#ffffff"));

// The page's dark under the rail's collapsed icons, as Android TV's
// (RailFadeSolidUntil, RailFadeClearFrom): the background at 90 % to
// 88 px, thinning to nothing at 184, short of the pages' 192 px gutter.
add(
  "fade-left.png",
  64,
  4,
  `<defs><linearGradient id="g">${stops([[0, 0.9], [0.48, 0.9], [1, 0]], BG)}</linearGradient></defs><rect width="64" height="4" fill="url(#g)"/>`,
);

// The open rail's shadow on the page: black at 45 % to nothing.
add("shadow.png", 48, 4, `<defs><linearGradient id="g">${stops([[0, 0.45], [1, 0]])}</linearGradient></defs><rect width="48" height="4" fill="url(#g)"/>`);

mkdirSync(out, { recursive: true });
// Pictures this script made before and no longer makes go.
const made = new Set(pictures.map((p) => p.name));
for (const name of readdirSync(out)) {
  if (!made.has(name) && name.endsWith(".png")) rmSync(join(out, name));
}
const r = await renderer();
try {
  for (const p of pictures) writeFileSync(join(out, p.name), await r.png(p.svg, p.width, p.height));
  console.log(`rendered ${pictures.length} pictures into ${out}`);
} finally {
  r.close();
}
