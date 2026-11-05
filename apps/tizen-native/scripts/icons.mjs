// Writes the native Samsung app's icons, src/Pley.Tv.Tizen/shared/res/icons/<name>.svg, from
// Lucide (ISC, the icons the Android app and the web TV app draw), and Lucide's licence beside
// them. The icons are the ones packages/tv/src/ui/kit/icons.tsx imports. Each is drawn in white,
// and the app tints it. Run again when that list or Lucide changes; the output is committed.

import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const out = join(root, "apps", "tizen-native", "src", "Pley.Tv.Tizen", "shared", "res", "icons");
const require = createRequire(join(root, "packages", "tv", "package.json"));
const lucide = dirname(require.resolve("lucide/package.json"));

// The names the web TV app imports from Lucide: its list is the set the TV screens use.
const source = readFileSync(join(root, "packages", "tv", "src", "ui", "kit", "icons.tsx"), "utf8");
const imports = /import\s*\{([^}]+)\}\s*from\s*"lucide/.exec(source);
if (!imports) throw new Error("No Lucide import in icons.tsx");
// Type imports (`type IconNode`) aren't icons.
const names = imports[1].split(",").map((s) => s.trim()).filter((s) => s && !s.startsWith("type "));
// Icons of pages only the native app has yet: a drive's folders, videos and empty folder.
names.push("Usb", "Folder", "FolderOpen", "FileVideo", "CornerLeftUp");
// Icons the Android app draws where the web TV app has none: Diagnostics, Remove from My List,
// and Settings' passthrough formats, subtitle background and position, reduce motion and the web.
names.push("Activity", "BookmarkMinus", "Volume2", "Square", "ArrowUp", "Sparkles", "AppWindow");
// And requests': a decline, and TMDB refusing the server's key.
names.push("TicketX", "TriangleAlert");

const kebab = (name) => name.replace(/([a-z0-9])([A-Z])/g, "$1-$2").replace(/([A-Za-z])([0-9])/g, "$1-$2").toLowerCase();
const attrs = (o) => Object.entries(o).map(([k, v]) => ` ${k}="${String(v).replace(/"/g, "&quot;")}"`).join("");

// Drawn filled as well as stroked, as `<name>-filled.svg` beside the outline: a favourite's star on
// Live TV's buttons, filled as the Android app's and the web TV app's are.
const filled = ["Star"];

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
// Through Lucide's own exports, which hold its renamed icons under their old names too.
const all = await import(pathToFileURL(require.resolve("lucide")).href);
const write = (name, file, fill) => {
  const node = all[name];
  if (!Array.isArray(node)) throw new Error(`Lucide has no ${name}`);
  const body = node.map(([tag, a]) => `<${tag}${attrs(a)}/>`).join("");
  writeFileSync(
    join(out, file),
    `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="${fill}" stroke="#ffffff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</svg>\n`,
  );
};
for (const name of names) write(name, `${kebab(name)}.svg`, "none");
for (const name of filled) write(name, `${kebab(name)}-filled.svg`, "#ffffff");
writeFileSync(join(out, "LICENSE.txt"), readFileSync(join(lucide, "LICENSE"), "utf8"));
console.log(`${names.length + filled.length} icons in ${out}`);
