// Marks where to look on a screenshot for a pull request: red boxes and
// arrows, and a zoom on a detail a few pixels wide, pixel for pixel
// (docs/DEVELOPMENT.md, "Screenshots for a pull request"). Places are in the
// screenshot's own pixels, zoomed or not. Takes a PNG or a JPEG, writes a PNG.
//
//   bun scripts/mark-screenshot.mjs <in> <out.png> [--box x,y,w,h]... [--arrow x1,y1,x2,y2]... [--zoom x,y,w,h[,scale]]
//
//   --box x,y,w,h            a box round the part to look at
//   --arrow x1,y1,x2,y2      an arrow from the first point to the second
//   --zoom x,y,w,h[,scale]   only that part, scale times (5 unless given)
//
// Chrome comes from $CHROME, else the usual place on Windows, macOS or Linux
// (svg-render.mjs).

import { readFileSync, writeFileSync } from "node:fs";
import { renderer } from "./svg-render.mjs";

const RED = "#ff3b30";
// The marks' width in the picture written, zoomed or not.
const STROKE = 8;

const [input, output, ...rest] = process.argv.slice(2);
if (!input || !output) usage();
const boxes = [];
const arrows = [];
let zoom = null;
for (let i = 0; i < rest.length; i += 2) {
  const numbers = (rest[i + 1] ?? "").split(",").map(Number);
  if (numbers.some((n) => !Number.isFinite(n))) usage(`${rest[i]} takes numbers: ${rest[i + 1] ?? ""}`);
  if (rest[i] === "--box" && numbers.length === 4) boxes.push(numbers);
  else if (rest[i] === "--arrow" && numbers.length === 4) arrows.push(numbers);
  else if (rest[i] === "--zoom" && (numbers.length === 4 || numbers.length === 5)) zoom = { x: numbers[0], y: numbers[1], w: numbers[2], h: numbers[3], scale: numbers[4] ?? 5 };
  else usage(`Not understood: ${rest[i]} ${rest[i + 1] ?? ""}`);
}

const picture = readFileSync(input);
const { width, height, type } = sizeOf(picture);
const view = zoom ?? { x: 0, y: 0, w: width, h: height, scale: 1 };
const outWidth = Math.round(view.w * view.scale);
const outHeight = Math.round(view.h * view.scale);
// A place in the screenshot, in the picture written.
const px = (x) => (x - view.x) * view.scale;
const py = (y) => (y - view.y) * view.scale;

const marks = [
  ...boxes.map(([x, y, w, h]) => `<rect x="${px(x)}" y="${py(y)}" width="${w * view.scale}" height="${h * view.scale}" rx="10" fill="none" stroke="${RED}" stroke-width="${STROKE}"/>`),
  ...arrows.map(([x1, y1, x2, y2]) => `<line x1="${px(x1)}" y1="${py(y1)}" x2="${px(x2)}" y2="${py(y2)}" stroke="${RED}" stroke-width="${STROKE}" stroke-linecap="round" marker-end="url(#head)"/>`),
].join("");
const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${outWidth} ${outHeight}">
<defs><marker id="head" viewBox="0 0 10 10" refX="7" refY="5" markerWidth="4" markerHeight="4" orient="auto-start-reverse"><path d="M0 0 L10 5 L0 10 z" fill="${RED}"/></marker></defs>
<image x="${px(0)}" y="${py(0)}" width="${width * view.scale}" height="${height * view.scale}" style="image-rendering:pixelated" href="data:image/${type};base64,${picture.toString("base64")}"/>
${marks}</svg>`;

const r = await renderer();
try {
  writeFileSync(output, await r.png(svg, outWidth, outHeight));
} finally {
  r.close();
}
console.log(`${output}: ${outWidth}×${outHeight}`);

/** The picture's width and height from its header: a PNG's IHDR, a JPEG's start of frame. */
function sizeOf(bytes) {
  if (bytes.subarray(0, 8).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))) {
    return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20), type: "png" };
  }
  if (bytes[0] === 0xff && bytes[1] === 0xd8) {
    let at = 2;
    while (at + 9 < bytes.length && bytes[at] === 0xff) {
      const marker = bytes[at + 1];
      // SOF0 to SOF15, less DHT (C4), JPG (C8) and DAC (CC), which aren't frames.
      if (marker >= 0xc0 && marker <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(marker)) {
        return { width: bytes.readUInt16BE(at + 7), height: bytes.readUInt16BE(at + 5), type: "jpeg" };
      }
      at += 2 + bytes.readUInt16BE(at + 2);
    }
  }
  usage(`${input} isn't a PNG or a JPEG`);
}

function usage(problem) {
  if (problem) console.error(problem);
  console.error("bun scripts/mark-screenshot.mjs <in> <out.png> [--box x,y,w,h]... [--arrow x1,y1,x2,y2]... [--zoom x,y,w,h[,scale]]");
  process.exit(1);
}
