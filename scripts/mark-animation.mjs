// Turns a recording's frames (the native Samsung app's `metrics.mjs record`,
// the Android app's record-android.mjs) into an animated PNG for a pull request, marked as mark-screenshot.mjs marks
// a screenshot: red boxes and arrows, on every frame (docs/DEVELOPMENT.md,
// "Screenshots for a pull request"). Places are in the frames' own pixels.
// An animated PNG rather than a GIF: a GIF's 256 colours band a dark page's
// gradients.
//
//   bun scripts/mark-animation.mjs <frames folder> <out.png> [--box x,y,w,h]... [--arrow x1,y1,x2,y2]...
//                                  [--speed 0.2] [--scale 0.5] [--step 20] [--hold 800]
//
//   --speed s   played s times as fast as it was: 0.2 is five times slower (1 unless given)
//   --scale k   k times the frames' size (0.5 unless given)
//   --step ms   how long each frame written shows, 20 at the least (20 unless given): 40 halves
//               the size of one whose whole screen moves, as a crossfade
//   --hold ms   how long the first and the last frame stay (800 unless given)
//
// The frames are NNN-<ms>.jpg, ms when each was caught at the animation's own
// speed. Each step of the animation written shows the last frame caught by
// then: browsers play a frame shorter than 20 ms as 100 ms. Chrome comes from
// $CHROME, else the usual place on Windows, macOS or Linux (svg-render.mjs).

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { deflateSync, inflateSync } from "node:zlib";
import { renderer } from "./svg-render.mjs";

const RED = "#ff3b30";
// The marks' width in the frames' own pixels, as mark-screenshot.mjs draws them on a screenshot.
const STROKE = 8;

const [input, output, ...rest] = process.argv.slice(2);
if (!input || !output) usage();
const boxes = [];
const arrows = [];
let speed = 1;
let scale = 0.5;
let hold = 800;
let step = 20;
for (let i = 0; i < rest.length; i += 2) {
  const numbers = (rest[i + 1] ?? "").split(",").map(Number);
  if (numbers.some((n) => !Number.isFinite(n))) usage(`${rest[i]} takes numbers: ${rest[i + 1] ?? ""}`);
  if (rest[i] === "--box" && numbers.length === 4) boxes.push(numbers);
  else if (rest[i] === "--arrow" && numbers.length === 4) arrows.push(numbers);
  else if (rest[i] === "--speed" && numbers.length === 1 && numbers[0] > 0) speed = numbers[0];
  else if (rest[i] === "--scale" && numbers.length === 1 && numbers[0] > 0) scale = numbers[0];
  else if (rest[i] === "--step" && numbers.length === 1 && numbers[0] >= 20) step = numbers[0];
  else if (rest[i] === "--hold" && numbers.length === 1 && numbers[0] >= 0) hold = numbers[0];
  else usage(`Not understood: ${rest[i]} ${rest[i + 1] ?? ""}`);
}

const frames = readdirSync(input)
  .map((name) => ({ name, m: name.match(/^\d+-(\d+)\.jpg$/) }))
  .filter((f) => f.m)
  .map((f) => ({ file: join(input, f.name), at: Number(f.m[1]) }))
  .sort((a, b) => a.at - b.at);
if (frames.length === 0) usage(`No NNN-<ms>.jpg frames in ${input}`);

// The frames shown, each for as long as it is the last one caught.
const shown = [];
for (let t = frames[0].at; t <= frames[frames.length - 1].at; t += step * speed) {
  const frame = frames.findLast((f) => f.at <= t);
  const last = shown[shown.length - 1];
  if (last && last.frame === frame) last.ms += step;
  else shown.push({ frame, ms: step });
}
shown[0].ms += hold;
shown[shown.length - 1].ms += hold;

const { width, height } = jpegSize(readFileSync(frames[0].file));
const outWidth = Math.round(width * scale);
const outHeight = Math.round(height * scale);
const marks = [
  ...boxes.map(([x, y, w, h]) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="10" fill="none" stroke="${RED}" stroke-width="${STROKE}"/>`),
  ...arrows.map(([x1, y1, x2, y2]) => `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${RED}" stroke-width="${STROKE}" stroke-linecap="round" marker-end="url(#head)"/>`),
].join("");

const r = await renderer();
const pictures = [];
try {
  const drawn = new Map();
  for (const s of shown) {
    if (!drawn.has(s.frame)) {
      const jpeg = readFileSync(s.frame.file).toString("base64");
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}">
<defs><marker id="head" viewBox="0 0 10 10" refX="7" refY="5" markerWidth="4" markerHeight="4" orient="auto-start-reverse"><path d="M0 0 L10 5 L0 10 z" fill="${RED}"/></marker></defs>
<image width="${width}" height="${height}" href="data:image/jpeg;base64,${jpeg}"/>
${marks}</svg>`;
      drawn.set(s.frame, decodePng(await r.png(svg, outWidth, outHeight)));
    }
    pictures.push({ rgba: drawn.get(s.frame), ms: s.ms });
  }
} finally {
  r.close();
}
const png = animatedPng(pictures, outWidth, outHeight);
writeFileSync(output, png);
console.log(`${output}: ${outWidth}×${outHeight}, ${pictures.length} frames of ${frames.length}, ${Math.round(png.length / 1024)} KB`);

function usage(said) {
  if (said) console.error(said);
  console.error("bun scripts/mark-animation.mjs <frames folder> <out.png> [--box x,y,w,h]... [--arrow x1,y1,x2,y2]... [--speed 0.2] [--scale 0.5] [--step 20] [--hold 800]");
  process.exit(2);
}

/** A JPEG's size, from its first start-of-frame marker. */
function jpegSize(b) {
  for (let i = 2; i + 9 < b.length; ) {
    if (b[i] !== 0xff) break;
    const marker = b[i + 1];
    const length = b.readUInt16BE(i + 2);
    if (marker >= 0xc0 && marker <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(marker)) return { height: b.readUInt16BE(i + 5), width: b.readUInt16BE(i + 7) };
    i += 2 + length;
  }
  throw new Error("Not a JPEG, or one with no size");
}

/** Chrome's screenshot, an 8-bit RGB or RGBA PNG, as RGBA pixels. */
function decodePng(b) {
  let w = 0, h = 0, type = 0;
  const data = [];
  for (let i = 8; i < b.length; ) {
    const length = b.readUInt32BE(i);
    const kind = b.toString("latin1", i + 4, i + 8);
    const body = b.subarray(i + 8, i + 8 + length);
    if (kind === "IHDR") {
      w = body.readUInt32BE(0);
      h = body.readUInt32BE(4);
      type = body[9];
      if (body[8] !== 8 || (type !== 2 && type !== 6) || body[12] !== 0) throw new Error("Not an 8-bit RGB or RGBA PNG");
    } else if (kind === "IDAT") data.push(body);
    i += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(data));
  const bpp = type === 6 ? 4 : 3;
  const stride = w * bpp;
  const rows = Buffer.alloc(h * stride);
  for (let y = 0; y < h; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let x = 0; x < stride; x++) {
      const a = x >= bpp ? rows[y * stride + x - bpp] : 0;
      const up = y > 0 ? rows[(y - 1) * stride + x] : 0;
      const c = x >= bpp && y > 0 ? rows[(y - 1) * stride + x - bpp] : 0;
      const p = a + up - c;
      const pa = Math.abs(p - a), pb = Math.abs(p - up), pc = Math.abs(p - c);
      const paeth = pa <= pb && pa <= pc ? a : pb <= pc ? up : c;
      const add = [0, a, up, (a + up) >> 1, paeth][filter];
      rows[y * stride + x] = (line[x] + add) & 0xff;
    }
  }
  if (bpp === 4) return rows;
  const rgba = Buffer.alloc(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    rgba[i * 4] = rows[i * 3];
    rgba[i * 4 + 1] = rows[i * 3 + 1];
    rgba[i * 4 + 2] = rows[i * 3 + 2];
    rgba[i * 4 + 3] = 255;
  }
  return rgba;
}

/**
 * The pictures as one animated PNG: the first whole, each after it only where it differs from
 * the one before, which is most of the size saved when only part of the screen moves.
 */
function animatedPng(pictures, w, h) {
  const chunks = [Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])];
  const chunk = (kind, body) => {
    const head = Buffer.alloc(8);
    head.writeUInt32BE(body.length, 0);
    head.write(kind, 4, "latin1");
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), body])), 0);
    chunks.push(head, body, crc);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  chunk("IHDR", ihdr);
  const actl = Buffer.alloc(8);
  actl.writeUInt32BE(pictures.length, 0);
  chunk("acTL", actl);
  let seq = 0;
  pictures.forEach(({ rgba, ms }, n) => {
    const area = n === 0 ? { x: 0, y: 0, w, h } : changed(pictures[n - 1].rgba, rgba, w, h);
    const fctl = Buffer.alloc(26);
    fctl.writeUInt32BE(seq++, 0);
    fctl.writeUInt32BE(area.w, 4);
    fctl.writeUInt32BE(area.h, 8);
    fctl.writeUInt32BE(area.x, 12);
    fctl.writeUInt32BE(area.y, 16);
    // The delay is a 16-bit fraction: in ms up to 65.5 s, in coarser units past that.
    let delay = Math.round(ms);
    let unit = 1000;
    while (delay > 0xffff && unit > 1) {
      delay = Math.round(delay / 10);
      unit /= 10;
    }
    fctl.writeUInt16BE(Math.min(delay, 0xffff), 20);
    fctl.writeUInt16BE(unit, 22);
    chunk("fcTL", fctl);
    const data = deflateSync(filtered(rgba, w, area), { level: 9 });
    if (n === 0) chunk("IDAT", data);
    else {
      const seqd = Buffer.alloc(4);
      seqd.writeUInt32BE(seq++, 0);
      chunk("fdAT", Buffer.concat([seqd, data]));
    }
  });
  chunk("IEND", Buffer.alloc(0));
  return Buffer.concat(chunks);
}

/** The smallest area holding every pixel that differs between two pictures; one pixel when none does. */
function changed(before, after, w, h) {
  let x0 = w, y0 = h, x1 = -1, y1 = -1;
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const o = (y * w + x) * 4;
      if (before[o] === after[o] && before[o + 1] === after[o + 1] && before[o + 2] === after[o + 2]) continue;
      if (x < x0) x0 = x;
      if (x > x1) x1 = x;
      if (y < y0) y0 = y;
      if (y > y1) y1 = y;
    }
  return x1 < 0 ? { x: 0, y: 0, w: 1, h: 1 } : { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 };
}

/** An area of a picture as PNG rows, each with the filter that leaves it smallest. */
function filtered(rgba, w, area) {
  const stride = area.w * 4;
  const out = Buffer.alloc(area.h * (stride + 1));
  const row = (y) => rgba.subarray(((area.y + y) * w + area.x) * 4, ((area.y + y) * w + area.x) * 4 + stride);
  // Above the area's first row and left of its first pixel count as 0, as PNG's filters have it.
  const none = Buffer.alloc(stride);
  // The row under each of the five filters (none, sub, up, average, Paeth), all in one pass.
  const lines = Array.from({ length: 5 }, () => Buffer.alloc(stride));
  const sums = [0, 0, 0, 0, 0];
  const cost = (d) => (d < 128 ? d : 256 - d);
  for (let y = 0; y < area.h; y++) {
    const cur = row(y);
    const above = y > 0 ? row(y - 1) : none;
    sums.fill(0);
    for (let j = 0; j < stride; j++) {
      const v = cur[j], up = above[j];
      const a = j >= 4 ? cur[j - 4] : 0, c = j >= 4 ? above[j - 4] : 0;
      const p = a + up - c;
      const pa = Math.abs(p - a), pb = Math.abs(p - up), pc = Math.abs(p - c);
      const paeth = pa <= pb && pa <= pc ? a : pb <= pc ? up : c;
      const d1 = (v - a) & 0xff, d2 = (v - up) & 0xff, d3 = (v - ((a + up) >> 1)) & 0xff, d4 = (v - paeth) & 0xff;
      lines[0][j] = v;
      lines[1][j] = d1;
      lines[2][j] = d2;
      lines[3][j] = d3;
      lines[4][j] = d4;
      sums[0] += cost(v);
      sums[1] += cost(d1);
      sums[2] += cost(d2);
      sums[3] += cost(d3);
      sums[4] += cost(d4);
    }
    let best = 0;
    for (let f = 1; f < 5; f++) if (sums[f] < sums[best]) best = f;
    out[y * (stride + 1)] = best;
    lines[best].copy(out, y * (stride + 1) + 1);
  }
  return out;
}

function crc32(b) {
  let c = ~0;
  for (const byte of b) {
    c ^= byte;
    for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1;
  }
  return ~c >>> 0;
}
