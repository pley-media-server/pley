// Draws the README's picture of the apps (docs/images/apps.webp): the TV app, with the phone app in
// front of its corner, on a transparent background, from a screenshot of each (a 1920 × 1080 TV and
// the pley_test emulator's 1080 × 2280 phone). Usage: bun devices.mjs <tv screenshot> <phone screenshot>.
// It needs ffmpeg on PATH.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { renderer } from "../svg-render.mjs";

const [tv, phone] = process.argv.slice(2);
if (!tv || !phone) throw new Error("usage: bun devices.mjs <tv screenshot> <phone screenshot>");
const out = join(import.meta.dir, "../../docs/images/apps.webp");
const picture = (file) => `data:image/${file.endsWith(".png") ? "png" : "jpeg"};base64,${readFileSync(file).toString("base64")}`;

/** The part [sx], [sy], [sw] × [sh] of the screenshot [id], stretched over [x], [y], [w] × [h]. */
const part = (id, [sx, sy, sw, sh], [x, y, w, h]) =>
  `<svg x="${x}" y="${y}" width="${w}" height="${h}" viewBox="${sx} ${sy} ${sw} ${sh}" preserveAspectRatio="none"><use href="#${id}"/></svg>`;

/** A dark frame [pad] wide around a screen at [x], [y], [w] × [h], showing [content]. */
function device(id, content, { x, y, w, h, pad, frame, corner, edge }) {
  return `
    <rect x="${x - pad}" y="${y - pad}" width="${w + 2 * pad}" height="${h + 2 * pad}" rx="${frame}"
      fill="#08090c" stroke="${edge}" stroke-width="2" filter="url(#shadow)"/>
    <clipPath id="${id}-screen"><rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${corner}"/></clipPath>
    <g clip-path="url(#${id}-screen)">${content}</g>`;
}

const tvAt = { x: 62, y: 36, w: 1616, h: 909, pad: 12, frame: 14, corner: 4, edge: "#323b50" };
const phoneAt = { x: 1452, y: 228, w: 360, h: 760, pad: 12, frame: 45, corner: 35, edge: "#363f55" };
const px = phoneAt.w / 1080;
// The phone's status bar (its top 80 rows) and gesture bar (its last 60) are Android's, not Pley's:
// each is painted over with one empty column of itself, as their backgrounds are the same across.
const phoneScreen = [
  part("phone", [0, 0, 1080, 2280], [phoneAt.x, phoneAt.y, phoneAt.w, phoneAt.h]),
  part("phone", [540, 0, 1, 80], [phoneAt.x, phoneAt.y, phoneAt.w, 80 * px]),
  part("phone", [100, 2220, 1, 60], [phoneAt.x, phoneAt.y + 2220 * px, phoneAt.w, 60 * px]),
].join("");

const width = 1884;
const height = 1060;
const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}">
  <defs>
    <image id="tv" href="${picture(tv)}" width="1920" height="1080"/>
    <image id="phone" href="${picture(phone)}" width="1080" height="2280"/>
    <filter id="shadow" x="-10%" y="-10%" width="120%" height="125%">
      <feDropShadow dx="0" dy="24" stdDeviation="24" flood-opacity="0.25"/>
    </filter>
  </defs>
  ${device("tv", part("tv", [0, 0, 1920, 1080], [tvAt.x, tvAt.y, tvAt.w, tvAt.h]), tvAt)}
  ${device("phone", phoneScreen, phoneAt)}
</svg>`;

const render = await renderer();
try {
  const png = await render.png(svg, width, height);
  const ffmpeg = spawnSync(
    "ffmpeg",
    ["-hide_banner", "-loglevel", "error", "-y", "-i", "-", "-vf", "scale=1600:-1:flags=lanczos,format=yuva420p", "-c:v", "libwebp", "-quality", "82", out],
    { input: png, stdio: ["pipe", "inherit", "inherit"] },
  );
  if (ffmpeg.status !== 0) throw new Error("ffmpeg couldn't make the picture");
  console.log(`the picture is in ${out}`);
} finally {
  render.close();
}
