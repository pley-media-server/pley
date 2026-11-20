// Draws the README's picture of the apps (docs/images/apps.webp): the TV app, with the phone app in
// front of its corner, on a transparent background, from a screenshot of each (1920 × 1080 and
// 1080 × 1920). Usage: bun devices.mjs <tv screenshot> <phone screenshot>. It needs ffmpeg on PATH.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { renderer } from "../svg-render.mjs";

const [tv, phone] = process.argv.slice(2);
if (!tv || !phone) throw new Error("usage: bun devices.mjs <tv screenshot> <phone screenshot>");
const out = join(import.meta.dir, "../../docs/images/apps.webp");
const picture = (file) => `data:image/${file.endsWith(".png") ? "png" : "jpeg"};base64,${readFileSync(file).toString("base64")}`;

/** A dark frame [pad] wide around [file], shown at [x], [y], [w] × [h]. */
function device(id, file, { x, y, w, h, pad, frame, corner, edge }) {
  return `
    <rect x="${x - pad}" y="${y - pad}" width="${w + 2 * pad}" height="${h + 2 * pad}" rx="${frame}"
      fill="#08090c" stroke="${edge}" stroke-width="2" filter="url(#shadow)"/>
    <clipPath id="${id}"><rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${corner}"/></clipPath>
    <image href="${picture(file)}" x="${x}" y="${y}" width="${w}" height="${h}" preserveAspectRatio="xMidYMid slice" clip-path="url(#${id})"/>`;
}

const width = 1930;
const height = 1060;
const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}">
  <filter id="shadow" x="-10%" y="-10%" width="120%" height="125%">
    <feDropShadow dx="0" dy="24" stdDeviation="24" flood-opacity="0.25"/>
  </filter>
  ${device("tv", tv, { x: 62, y: 36, w: 1616, h: 909, pad: 12, frame: 14, corner: 4, edge: "#323b50" })}
  ${device("phone", phone, { x: 1452, y: 266, w: 406, h: 722, pad: 12, frame: 50, corner: 39, edge: "#363f55" })}
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
