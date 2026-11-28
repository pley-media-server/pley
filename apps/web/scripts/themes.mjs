// Writes packages/themes/themes.json from index.css: the themes and accents
// every app draws (src/theme/spec.ts). `bun run themes` after a color changes;
// src/theme/spec.test.ts fails until it is run.
import { readFileSync, writeFileSync } from "node:fs";
import { themeSpec } from "../src/theme/spec.ts";

const here = (path) => new URL(path, import.meta.url);
const css = readFileSync(here("../src/index.css"), "utf8");
// The picker's order, as src/theme lists them.
const theme = readFileSync(here("../src/theme/index.ts"), "utf8");
const list = (name) => JSON.parse(new RegExp(`export const ${name} = (\\[[^\\]]*\\]) as const;`).exec(theme)[1]);
const spec = themeSpec(css, list("THEMES"), list("ACCENTS"));
writeFileSync(here("../../../packages/themes/themes.json"), `${JSON.stringify(spec, null, 2)}\n`);
console.log(`${spec.themes.length} themes, ${spec.accents.length} accents`);
