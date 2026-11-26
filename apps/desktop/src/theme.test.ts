import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import { colorsOf, pickFrom, showPick } from "./theme";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

interface Spec {
  themes: { id: string; kind: "dark" | "light"; colors: Record<string, string>; follows: string[] }[];
  accents: ({ id: string } & Record<"dark" | "light", Record<string, string>>)[];
}

const spec = JSON.parse(read("../../../packages/themes/themes.json")) as Spec;
const css = read("./style.css");
const webCss = read("../../web/src/index.css");

describe("the picker's colors", () => {
  it("are the file's for every theme and accent", () => {
    let checked = 0;
    for (const theme of spec.themes) {
      for (const accent of [null, ...spec.accents]) {
        const colors = colorsOf({ theme: theme.id, accent: accent?.id ?? null });
        const want = Object.fromEntries(
          Object.entries(theme.colors).map(([token, own]) => [token, theme.follows.includes(token) ? (accent?.[theme.kind][token] ?? own) : own]),
        );
        expect(colors, `${theme.id} with ${accent?.id ?? "its own"}`).toEqual(want);
        checked += Object.keys(want).length;
      }
    }
    expect(checked).toBe(9 * 7 * 23);
  });

  it("keep Sunshine's logo and Blacklight's states under an accent", () => {
    expect(colorsOf({ theme: "sunshine", accent: "violet" })).toMatchObject({ brand: "#ff8a1f", accent: "#5a2ff0" });
    expect(colorsOf({ theme: "blacklight", accent: "mint" })).toMatchObject({ success: "#e0e0e0", accent: "#5fd39a" });
    expect(colorsOf({ theme: "midnight", accent: null })).toMatchObject({ bg: "#0b0d10", accent: "#38bdf8" });
  });

  it("are Midnight's and the theme's own for ids the app doesn't know", () => {
    expect(pickFrom("")).toEqual({ theme: "midnight", accent: null });
    expect(pickFrom("?pick&theme=sunshine&accent=violet")).toEqual({ theme: "sunshine", accent: "violet" });
    expect(pickFrom("?theme=dusk&accent=violet")).toEqual({ theme: "midnight", accent: "violet" });
    expect(pickFrom("?theme=starlight&accent=teal")).toEqual({ theme: "starlight", accent: null });
  });
});

describe("showPick", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("puts every token on the page, with the theme's id and kind", () => {
    const meta = { setAttribute: vi.fn() };
    vi.stubGlobal("document", { querySelector: () => meta });
    const properties = new Map<string, string>();
    const root = {
      style: { setProperty: (name: string, value: string) => properties.set(name, value), colorScheme: "" },
      dataset: {} as Record<string, string>,
    };
    showPick({ theme: "midday", accent: "miami" }, root as unknown as HTMLElement);
    expect(properties.get("--accent")).toBe("#a61e6a");
    expect(properties.get("--brand-tile")).toBe("#ddeefa");
    expect(properties.get("--halo")).toBe("rgb(166 30 106 / 0.09)");
    expect(properties.size).toBe(24);
    expect(root.dataset.theme).toBe("midday");
    expect(root.style.colorScheme).toBe("light");
    expect(meta.setAttribute).toHaveBeenCalledWith("content", "light");
  });
});

/** The value of `--glow` in the block of `selector`, its spaces made one. */
function glow(text: string, selector: (item: string) => boolean): string | undefined {
  for (const [, selectors, body] of text.matchAll(/([^{}]*)\{([^{}]*)\}/g)) {
    const items = selectors!.replace(/\/\*[^]*?\*\//g, "").split(",");
    if (!items.some((item) => selector(item.trim()))) continue;
    const value = /--glow:\s*([^;]+);/.exec(body!)?.[1];
    if (value) return value.replace(/\s+/g, " ").trim();
  }
  return undefined;
}

describe("style.css", () => {
  it("draws the web app's backdrops", () => {
    for (const { id } of spec.themes) {
      const web = glow(webCss, (item) => item === `[data-theme="${id}"]`);
      const ours = glow(css, (item) => item === `:root[data-theme="${id}"]`);
      // A theme without one has the picker's light at the top, in the accent.
      expect(ours, id).toEqual(web === "none" ? undefined : web);
    }
  });

  it("draws in the theme's colors only", () => {
    const tokens = new Set([...Object.keys(spec.themes[0]!.colors), "halo", "glow", "radius", "radius-sm"]);
    const used = [...css.matchAll(/var\(--([\w-]+)/g)].map((m) => m[1]!);
    expect(used.filter((name) => !tokens.has(name))).toEqual([]);
    // Colors written out are the backdrops' alone, held to the web's above.
    const rest = css.replace(/\/\*[^]*?\*\//g, "").replace(/--glow:[^;]+;/g, "");
    expect(rest).not.toMatch(/#[0-9a-f]{3,8}\b|\b(rgba?|hsla?|oklch|color-mix)\(|(?<![\w-])(white|black)(?![\w-])/i);
    expect(read("../index.html")).not.toMatch(/#[0-9a-f]{3,8}\b|(fill|stop-color)="(?!url)/i);
  });
});
