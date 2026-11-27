// Checks what Roku's certification asks to be broadcast-safe (its
// requirement 6.4: the store artwork and the splash screen) against the
// range bslint's color-cert rule uses: every color's luma (BT.709, on its
// 0-255 channels) no darker than #161616's and no lighter than #EBEBEB's.
// That is the manifest's splash_color, and every pixel of the splash
// screens and the channel's poster artwork (the manifest's splash_screen_*
// and mm_icon_focus_*). The app's own screens draw the themes' exact
// colors. scripts/test.mjs runs it before the unit tests.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { inflateSync } from "node:zlib";

const WEIGHTS = [0.2126, 0.7152, 0.0722];
const luma = (r, g, b) => WEIGHTS[0] * r + WEIGHTS[1] * g + WEIGHTS[2] * b;
const DARKEST = luma(0x16, 0x16, 0x16);
const LIGHTEST = luma(0xeb, 0xeb, 0xeb);

/** A PNG's pixels ({ width, height, channels, data }), 8-bit grayscale, RGB or RGBA. */
export function readPng(file) {
  const bytes = readFileSync(file);
  let at = 8;
  let width = 0;
  let height = 0;
  let type = 0;
  const idat = [];
  while (at < bytes.length) {
    const length = bytes.readUInt32BE(at);
    const kind = bytes.toString("latin1", at + 4, at + 8);
    const body = bytes.subarray(at + 8, at + 8 + length);
    if (kind === "IHDR") {
      width = body.readUInt32BE(0);
      height = body.readUInt32BE(4);
      if (body[8] !== 8 || body[12] !== 0) throw new Error(`${file}: only 8-bit, non-interlaced PNGs`);
      type = body[9];
    } else if (kind === "IDAT") idat.push(body);
    at += 12 + length;
  }
  const channels = { 0: 1, 2: 3, 4: 2, 6: 4 }[type];
  if (!channels) throw new Error(`${file}: color type ${type}`);
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const data = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let x = 0; x < stride; x++) {
      const left = x >= channels ? data[y * stride + x - channels] : 0;
      const up = y > 0 ? data[(y - 1) * stride + x] : 0;
      const corner = x >= channels && y > 0 ? data[(y - 1) * stride + x - channels] : 0;
      let value = line[x];
      if (filter === 1) value += left;
      else if (filter === 2) value += up;
      else if (filter === 3) value += (left + up) >> 1;
      else if (filter === 4) {
        const p = left + up - corner;
        const [pa, pb, pc] = [Math.abs(p - left), Math.abs(p - up), Math.abs(p - corner)];
        value += pa <= pb && pa <= pc ? left : pb <= pc ? up : corner;
      }
      data[y * stride + x] = value & 255;
    }
  }
  return { width, height, channels, data };
}

/** Problems with the splash and the artwork: what falls outside the range. */
export function checkBroadcastSafe(root) {
  const problems = [];
  const manifest = Object.fromEntries(
    readFileSync(join(root, "manifest"), "utf8")
      .split(/\r?\n/)
      .filter((line) => line.includes("="))
      .map((line) => [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1)]),
  );
  const color = /^#([0-9a-f]{6})$/i.exec(manifest.splash_color ?? "");
  if (!color) problems.push("manifest: no splash_color");
  else {
    const [r, g, b] = [0, 2, 4].map((i) => parseInt(color[1].slice(i, i + 2), 16));
    const l = luma(r, g, b);
    if (l < DARKEST || l > LIGHTEST) problems.push(`manifest: splash_color ${manifest.splash_color} has luma ${l.toFixed(1)}`);
  }
  const pictures = Object.entries(manifest).filter(([key]) => key.startsWith("splash_screen_") || key.startsWith("mm_icon_focus_"));
  if (pictures.length === 0) problems.push("manifest: no splash screens or poster artwork");
  for (const [key, uri] of pictures) {
    const file = join(root, uri.replace(/^pkg:\//, ""));
    const { width, height, channels, data } = readPng(file);
    let outside = 0;
    let worst = "";
    for (let i = 0; i < width * height; i++) {
      const at = i * channels;
      // A pixel fully clear shows nothing of its own.
      if (channels === 4 && data[at + 3] === 0) continue;
      if (channels === 2 && data[at + 1] === 0) continue;
      const [r, g, b] = channels >= 3 ? [data[at], data[at + 1], data[at + 2]] : [data[at], data[at], data[at]];
      const l = luma(r, g, b);
      if (l < DARKEST || l > LIGHTEST) {
        outside++;
        if (!worst) worst = `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")} at ${i % width},${Math.floor(i / width)}`;
      }
    }
    if (outside > 0) problems.push(`${key} (${uri}): ${outside} pixels outside the range, the first ${worst}`);
  }
  return problems;
}
