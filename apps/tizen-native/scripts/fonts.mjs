// Writes the native Samsung app's fonts, src/Pley.Tv.Tizen/shared/res/fonts/roboto-<weight>.ttf:
// Roboto (OFL-1.1), the face the Android app's type uses, from the same @fontsource/roboto files
// the web TV app ships (latin, weights 400, 500 and 700), and its licence beside them. NUI loads
// TrueType, so each WOFF is unpacked to the font it holds; nothing in the font changes. Run again
// when @fontsource/roboto changes; the output is committed.

import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const out = join(root, "apps", "tizen-native", "src", "Pley.Tv.Tizen", "shared", "res", "fonts");
const require = createRequire(join(root, "packages", "tv", "package.json"));
const roboto = dirname(require.resolve("@fontsource/roboto/package.json"));

/** The TrueType font a WOFF 1.0 file holds (https://www.w3.org/TR/WOFF/): its tables, inflated. */
function unpack(woff) {
  if (woff.readUInt32BE(0) !== 0x774f4646) throw new Error("not a WOFF file");
  const flavor = woff.readUInt32BE(4);
  const count = woff.readUInt16BE(12);
  const tables = [];
  for (let i = 0; i < count; i++) {
    const at = 44 + i * 20;
    const offset = woff.readUInt32BE(at + 4);
    const compressed = woff.readUInt32BE(at + 8);
    const length = woff.readUInt32BE(at + 12);
    const raw = woff.subarray(offset, offset + compressed);
    const data = compressed < length ? inflateSync(raw) : raw;
    if (data.length !== length) throw new Error("a table didn't unpack to its length");
    tables.push({ tag: woff.readUInt32BE(at), checksum: woff.readUInt32BE(at + 16), data });
  }
  const pad = (n) => (n + 3) & ~3;
  let size = 12 + count * 16;
  for (const t of tables) size += pad(t.data.length);
  const font = Buffer.alloc(size);
  let power = 1;
  while (power * 2 <= count) power *= 2;
  font.writeUInt32BE(flavor, 0);
  font.writeUInt16BE(count, 4);
  font.writeUInt16BE(power * 16, 6);
  font.writeUInt16BE(Math.log2(power), 8);
  font.writeUInt16BE(count * 16 - power * 16, 10);
  let offset = 12 + count * 16;
  tables.forEach((t, i) => {
    const at = 12 + i * 16;
    font.writeUInt32BE(t.tag, at);
    font.writeUInt32BE(t.checksum, at + 4);
    font.writeUInt32BE(offset, at + 8);
    font.writeUInt32BE(t.data.length, at + 12);
    t.data.copy(font, offset);
    offset += pad(t.data.length);
  });
  return font;
}

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
for (const weight of [400, 500, 700]) {
  const woff = readFileSync(join(roboto, "files", `roboto-latin-${weight}-normal.woff`));
  writeFileSync(join(out, `roboto-${weight}.ttf`), unpack(woff));
}
writeFileSync(join(out, "LICENSE.txt"), readFileSync(join(roboto, "LICENSE"), "utf8"));
console.log(`Roboto 400, 500 and 700 in ${out}`);
