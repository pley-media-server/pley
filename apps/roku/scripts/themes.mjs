// Writes components/lib/ThemeTable.brs from packages/themes/themes.json,
// where every app's themes and accents come from (docs/THEMES.md, "In every
// app"): each theme's kind, twin, own colors and the tokens an accent
// replaces, and each accent's colors for the dark themes and the light
// ones. Colors become Roku's, exactly the JSON's ("0x0B0D10FF", as
// bslint's color-format, color-case and color-alpha rules want them), and
// tokens camelCase ("surface-2" is surface2). tests/themes.test.brs holds
// the app's colors to the JSON, so run this after it changes.
//
//   bun run themes

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const spec = JSON.parse(readFileSync(join(root, "../../packages/themes/themes.json"), "utf8"));

const camel = (token) => token.replace(/-([a-z0-9])/g, (_, c) => c.toUpperCase());

/** A CSS color of the JSON's ("#0b0d10", "rgb(16 26 44 / 0.42)") as Roku's ("0x0B0D10FF"). */
function roku(color) {
  const hex = /^#([0-9a-f]{6})$/i.exec(color);
  if (hex) return `0x${hex[1].toUpperCase()}FF`;
  const rgb = /^rgb\(\s*(\d+)\s+(\d+)\s+(\d+)\s*\/\s*([\d.]+)\s*\)$/.exec(color);
  if (!rgb) throw new Error(`a color this script can't read: ${color}`);
  const byte = (n) => Number(n).toString(16).padStart(2, "0").toUpperCase();
  return "0x" + byte(rgb[1]) + byte(rgb[2]) + byte(rgb[3]) + byte(Math.round(Number(rgb[4]) * 255));
}

const colors = (set, indent) =>
  Object.entries(set)
    .map(([token, color]) => `${indent}${camel(token)}: "${roku(color)}"`)
    .join(",\n");

const lines = [
  "' Pley's themes and accents as packages/themes/themes.json has them,",
  "' written by scripts/themes.mjs (bun run themes): tests/themes.test.brs",
  "' fails while this differs from the JSON. Colors are Roku's, the JSON's",
  "' own; tokens are camelCase.",
  "",
  "' The themes' ids, in the picker's order.",
  "function themeIds() as object",
  `    return [${spec.themes.map((t) => `"${t.id}"`).join(", ")}]`,
  "end function",
  "",
  "' The accents' ids, in the picker's order (the theme's own before them).",
  "function themeAccentIds() as object",
  `    return [${spec.accents.map((a) => `"${a.id}"`).join(", ")}]`,
  "end function",
  "",
  "' Each theme: dark or light, the dark theme its players draw in (\"\" for a",
  "' dark one), the tokens an accent replaces and its own colors.",
  "function themeTable() as object",
  "    return {",
  spec.themes
    .map((t) =>
      [
        `        ${t.id}: {`,
        `            kind: "${t.kind}",`,
        `            twin: "${t.twin ?? ""}",`,
        `            follows: [${t.follows.map((f) => `"${camel(f)}"`).join(", ")}],`,
        "            colors: {",
        colors(t.colors, "                "),
        "            }",
        "        }",
      ].join("\n"),
    )
    .join(",\n"),
  "    }",
  "end function",
  "",
  "' Each accent: the colors it gives the tokens a theme follows, for the dark",
  "' themes and for the light ones.",
  "function themeAccentTable() as object",
  "    return {",
  spec.accents
    .map((a) =>
      [
        `        ${a.id}: {`,
        "            dark: {",
        colors(a.dark, "                "),
        "            },",
        "            light: {",
        colors(a.light, "                "),
        "            }",
        "        }",
      ].join("\n"),
    )
    .join(",\n"),
  "    }",
  "end function",
  "",
];

writeFileSync(join(root, "components/lib/ThemeTable.brs"), lines.join("\n"));
console.log(`${spec.themes.length} themes and ${spec.accents.length} accents written to components/lib/ThemeTable.brs`);
