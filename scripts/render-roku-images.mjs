#!/usr/bin/env bun
// Renders the pictures Pley's Roku screens are drawn with
// (apps/roku/images/ui), in the Android TV app's look: its Lucide icons,
// read from the Android app's own set (PleyIcons.kt), and the shapes its
// controls are made of. Sizes are the Android TV app's dp doubled, as a
// Roku's 1080-line screen is twice its 540.
//
// The pictures are white, for the screens to tint in the theme's colors (a
// Poster's blendColor); what sits on a picture keeps black, which no tint
// changes. Nine-patch pictures (.9.png) carry Roku's one-pixel border:
// black on the top and left edges marks what stretches. Pley's logo comes
// as a picture per shape (Logo.tsx's), and the themes' backdrops (the
// web's --glow and --sidebar in apps/web/src/index.css) as pictures in
// their colors.
//
//   bun scripts/render-roku-images.mjs

import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { renderer } from "./svg-render.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "apps/roku/images/ui");

const WHITE = "#ffffff";

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
// Appearance in Settings, as the web's (Lucide's palette), its dots as round caps.
ICONS.palette ??= {
  filled: false,
  paths: [
    "M12 22a1 1 0 0 1 0-20 10 9 0 0 1 10 9 5 5 0 0 1-5 5h-2.25a1.75 1.75 0 0 0-1.4 2.8l.3.4a1.75 1.75 0 0 1-1.4 2.8z",
    "M13.5 6.5h.01",
    "M17.5 10.5h.01",
    "M6.5 12.5h.01",
    "M8.5 7.5h.01",
  ],
};
// Its Accent row: Lucide's droplet.
ICONS.droplet ??= { filled: false, paths: ["M12 22a7 7 0 0 0 7-7c0-2-1-3.9-3-5.5s-3.5-4-4-6.5c-.5 2.5-2 4.9-4 6.5C6 11.1 5 13 5 15a7 7 0 0 0 7 7z"] };

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
  "app-window", "arrow-left", "palette", "droplet",
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

// The scrubber's thumb: 20 px in fg; focused, 24 px (this one again) with
// an accent-hover ring 4 px out, 4 px wide (36 across).
add("thumb.png", 20, 20, `<circle cx="10" cy="10" r="10" fill="#ffffff"/>`);
add("thumb-ring.png", 36, 36, `<circle cx="18" cy="18" r="16" fill="none" stroke="#ffffff" stroke-width="4"/>`);

// Buffering: an 80 px ring, 4 px in fg, under a quarter in the accent, which
// the player turns.
add("buffering-track.png", 80, 80, `<circle cx="40" cy="40" r="38" fill="none" stroke="#ffffff" stroke-width="4"/>`);
add("buffering-arc.png", 80, 80, `<path d="M40 2A38 38 0 0 1 78 40" fill="none" stroke="#ffffff" stroke-width="4" stroke-linecap="round"/>`);

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

// The focus ring around a tile: a 6 px accent-bright stroke against the
// tile's edge, drawn 6 px out from it all round, its inner corners the
// tile's 8 px; nine-patch, its corners left whole. Android TV's stands 3 dp
// off the tile, a gap that on a Roku's grown tiles read as loose and
// reached the next tile.
ninePatch("ring.9.png", 64, 64, 22, `<rect x="3" y="3" width="58" height="58" rx="11" fill="none" stroke="#ffffff" stroke-width="6"/>`);
// The same ring around a person's circle, as wide as a row's (192 px and
// 6 px each side).
add("ring-round.png", 204, 204, `<circle cx="102" cy="102" r="99" fill="none" stroke="#ffffff" stroke-width="6"/>`);
// A profile's face in "Who's watching?" (220 px, to tint), and its focus
// ring, the same stroke 12 px out all round.
add("face.png", 220, 220, `<circle cx="110" cy="110" r="110" fill="#ffffff"/>`);
add("ring-face.png", 244, 244, `<circle cx="122" cy="122" r="119" fill="none" stroke="#ffffff" stroke-width="6"/>`);

// The line along a tile's inner edge (white at 6 %), corners 8.
ninePatch("hairline.9.png", 32, 32, 10, `<rect x="1" y="1" width="30" height="30" rx="7" fill="none" stroke="rgba(255,255,255,0.06)" stroke-width="2"/>`);

// The watched tick on a picture: black at 80 %, corners 4, and a white check
// the tile tints in the accent (the black stays black).
add(
  "tick.png",
  40,
  40,
  box(40, 40, 4, "rgba(0,0,0,0.8)") + `<path d="M28 14 17 25l-5-5" fill="none" stroke="#ffffff" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>`,
);

// The backdrop's scrims, in white for the theme's background: from the left
// (100 % at 0, 92 % at 600 px, 85 % at 1120, nothing at 1600 of the
// screen), and from clear to solid downwards, stretched as each page needs.
add(
  "scrim-left.png",
  800,
  4,
  `<defs><linearGradient id="g">${stops([[0, 1], [0.375, 0.92], [0.7, 0.85], [1, 0]], WHITE)}</linearGradient></defs><rect width="800" height="4" fill="url(#g)"/>`,
);
add(
  "fade-down.png",
  4,
  256,
  `<defs><linearGradient id="g" x2="0" y2="1">${stops([[0, 0], [1, 1]], WHITE)}</linearGradient></defs><rect width="4" height="256" fill="url(#g)"/>`,
);

// A pill's edge (TvPillButton, TvRoundButton at rest): 2 px, corners 12,
// white for the control color.
ninePatch("edge.9.png", 48, 48, 12, `<rect x="1" y="1" width="46" height="46" rx="11" fill="none" stroke="#ffffff" stroke-width="2"/>`);

// A badge's edge on a page (the content rating, the quality): 2 px,
// corners 4, white for the outline's color.
ninePatch("edge4.9.png", 16, 16, 4, `<rect x="1" y="1" width="14" height="14" rx="3" fill="none" stroke="#ffffff" stroke-width="2"/>`);

// A circle to tint: the rail's avatar.
add("circle.png", 80, 80, `<circle cx="40" cy="40" r="40" fill="#ffffff"/>`);
// A round button's edge, as edge.9.png's but a circle (72 px across).
add("edge-round.png", 72, 72, `<circle cx="36" cy="36" r="35" fill="none" stroke="#ffffff" stroke-width="2"/>`);
// A settings switch's track (TvSwitch), tinted by the row.
add("switch.png", 80, 44, box(80, 44, 22, "#ffffff"));

// The page's dark under the rail's collapsed icons, as Android TV's
// (RailFadeSolidUntil, RailFadeClearFrom): the background (white, tinted)
// at 90 % to 88 px, thinning to nothing at 184, short of the pages' 192 px
// gutter.
add(
  "fade-left.png",
  64,
  4,
  `<defs><linearGradient id="g">${stops([[0, 0.9], [0.48, 0.9], [1, 0]], WHITE)}</linearGradient></defs><rect width="64" height="4" fill="url(#g)"/>`,
);

// The open rail's shadow on the page: white (the theme's shade, tinted) at
// 45 % to nothing.
add("shadow.png", 48, 4, `<defs><linearGradient id="g">${stops([[0, 0.45], [1, 0]], WHITE)}</linearGradient></defs><rect width="48" height="4" fill="url(#g)"/>`);

// Pley's logo (apps/web/src/components/Logo.tsx), a picture per shape on
// its 64 grid, 160 px, for the app to tint: the tile, the back and middle
// posters, and the front one with play cut out. Its gradient from
// brand-light to brand is the front in brand under the front again in
// brand-light, fading out along the web's gradient (logo-front-light.png).
const logo = readFileSync(join(root, "apps/web/src/components/Logo.tsx"), "utf8");
const shape = (name) => {
  const found = new RegExp(`const ${name} =\\s*"([^"]+)"`).exec(logo);
  if (!found) throw new Error(`no ${name} in Logo.tsx`);
  return found[1];
};
const mark = (body) => `<g transform="scale(2.5)">${body}</g>`;
add("logo-tile.png", 160, 160, mark(`<rect width="64" height="64" rx="15" fill="#ffffff"/>`));
add("logo-far.png", 160, 160, mark(`<path fill="#ffffff" d="${shape("FAR")}"/>`));
add("logo-deep.png", 160, 160, mark(`<path fill="#ffffff" d="${shape("DEEP")}"/>`));
add("logo-front.png", 160, 160, mark(`<path fill="#ffffff" fill-rule="evenodd" d="${shape("FRONT")}"/>`));
add(
  "logo-front-light.png",
  160,
  160,
  mark(
    `<defs><linearGradient id="g" x1="12" y1="15" x2="34" y2="49" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#ffffff" stop-opacity="0"/></linearGradient></defs>` +
      `<path fill="url(#g)" fill-rule="evenodd" d="${shape("FRONT")}"/>`,
  ),
);

// The themes' backdrops, as the web draws them behind its pages (--glow,
// on the theme's bg) and Sunshine's rainbow over its sidebar (--sidebar,
// clear, over the open rail's surface): CSS drawn by the browser itself, at
// half the screen, which a Poster stretches.
const css = readFileSync(join(root, "apps/web/src/index.css"), "utf8");
const themes = JSON.parse(readFileSync(join(root, "packages/themes/themes.json"), "utf8")).themes;
// A theme's own block, as spec.ts finds it: the one whose selectors name it alone on a line.
const blocks = [...css.matchAll(/([^{}]*)\{([^{}]*)\}/g)].map(([, selector, body]) => ({ selector, body }));
const variable = (theme, name) => {
  const block = blocks.find(({ selector }) => new RegExp(`^\\s*\\[data-theme="${theme}"\\]\\s*,?\\s*$`, "m").test(selector));
  if (!block) throw new Error(`no block for ${theme} in index.css`);
  const value = new RegExp(`${name}:\\s*([^;]+);`).exec(block.body);
  return value ? value[1].trim() : "none";
};
const html = (width, height, background) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}"><foreignObject width="${width}" height="${height}"><div xmlns="http://www.w3.org/1999/xhtml" style="width:${width}px;height:${height}px;background:${background}"></div></foreignObject></svg>`;
for (const theme of themes) {
  const glow = variable(theme.id, "--glow");
  if (glow !== "none") pictures.push({ name: `backdrop-${theme.id}.png`, width: 960, height: 540, svg: html(960, 540, `${glow}, ${theme.colors.bg}`) });
  const sidebar = variable(theme.id, "--sidebar");
  if (sidebar !== "none") pictures.push({ name: `sidebar-${theme.id}.png`, width: 4, height: 540, svg: html(4, 540, sidebar) });
}

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
