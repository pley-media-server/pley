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

/** A dark frame [pad] wide around the picture [href], shown at [x], [y], [w] × [h], with [extra] drawn over it. */
function device(id, href, { x, y, w, h, pad, frame, corner, edge }, extra = "") {
  return `
    <rect x="${x - pad}" y="${y - pad}" width="${w + 2 * pad}" height="${h + 2 * pad}" rx="${frame}"
      fill="#08090c" stroke="${edge}" stroke-width="2" filter="url(#shadow)"/>
    <clipPath id="${id}"><rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${corner}"/></clipPath>
    <g clip-path="url(#${id})">
      <image id="${id}-shot" href="${href}" x="${x}" y="${y}" width="${w}" height="${h}" preserveAspectRatio="xMidYMid slice"/>${extra}
    </g>`;
}

const phoneAt = { x: 1452, y: 228, w: 360, h: 760, pad: 12, frame: 45, corner: 35, edge: "#363f55" };
// Android puts the status bar's time 101 pixels from the left edge but ends its icons 77 from the
// right; the icons move in by the difference so the two sit evenly. The bar's background is the
// same across its width, so the move leaves no seam.
const px = phoneAt.w / 1080;
const statusIcons = `
      <clipPath id="status-icons"><rect x="${phoneAt.x + 800 * px}" y="${phoneAt.y}" width="${280 * px}" height="${66 * px}"/></clipPath>
      <g clip-path="url(#status-icons)"><use href="#phone-shot" x="${-24 * px}"/></g>`;

const width = 1884;
const height = 1060;
const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}">
  <filter id="shadow" x="-10%" y="-10%" width="120%" height="125%">
    <feDropShadow dx="0" dy="24" stdDeviation="24" flood-opacity="0.25"/>
  </filter>
  ${device("tv", picture(tv), { x: 62, y: 36, w: 1616, h: 909, pad: 12, frame: 14, corner: 4, edge: "#323b50" })}
  ${device("phone", picture(phone), phoneAt, statusIcons)}
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
