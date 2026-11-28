import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

interface Spec {
  themes: { id: string; colors: Record<string, string> }[];
}

const spec = JSON.parse(read("../../../packages/themes/themes.json")) as Spec;
const css = read("./style.css").replace(/\/\*[^]*?\*\//g, "");

/** The `--name: value` properties of style.css's :root. */
function rootProperties(): Map<string, string> {
  const root = /(?:^|\})\s*:root\s*\{([^{}]*)\}/.exec(css)?.[1] ?? "";
  return new Map([...root.matchAll(/--([\w-]+):\s*([^;]+);/g)].map(([, name, value]) => [name!, value!.trim()]));
}

// The picker shows outside any profile, where Pley draws Midnight in its
// own accent whatever a profile picked (docs/THEMES.md).
describe("the picker", () => {
  const midnight = spec.themes.find(({ id }) => id === "midnight")!.colors;

  it("draws in Midnight's colors from the file", () => {
    const properties = rootProperties();
    expect(properties.size).toBeGreaterThan(0);
    for (const [name, value] of properties) expect(value, name).toBe(midnight[name]);
  });

  it("draws no color but its properties and the light of Midnight's accent", () => {
    const defined = rootProperties();
    const used = [...css.matchAll(/var\(--([\w-]+)/g)].map((m) => m[1]!);
    expect(used.filter((name) => !defined.has(name))).toEqual([]);
    const [r, g, b] = [1, 3, 5].map((i) => parseInt(midnight.accent!.slice(i, i + 2), 16));
    const rest = css
      .replace(/(?:^|\})\s*:root\s*\{[^{}]*\}/, "")
      .replace(`rgb(${r} ${g} ${b} / 0.09)`, "");
    expect(rest).not.toMatch(/#[0-9a-f]{3,8}\b|\b(rgba?|hsla?|oklch|color-mix)\(|(?<![\w-])(white|black)(?![\w-])/i);
    expect(read("../index.html")).not.toMatch(/#[0-9a-f]{3,8}\b|(fill|stop-color)="(?!url)/i);
  });
});
