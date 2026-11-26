import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import type { KeyValue } from "../../src/auth/tokens";
import type { Platform } from "../../src/platform";
import { AppState } from "../../src/state";
import { accentId, ACCENTS, applyTheme, channels, ROLES, SUNSHINE_SIDEBAR, themeCss, themeId, THEMES, themeVariables, type AccentId, type ThemeId } from "../../src/ui/themes";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = join(HERE, "../../../..");

/** packages/themes/themes.json, read here apart from the app's own import of it: what every app draws. */
interface Spec {
  themes: { id: string; kind: "dark" | "light"; twin: string | null; colors: Record<string, string>; follows: string[] }[];
  accents: { id: string; dark: Record<string, string>; light: Record<string, string> }[];
}
const SPEC = JSON.parse(readFileSync(join(REPO, "packages/themes/themes.json"), "utf8")) as Spec;

/** A token's color as the spec has it (`colorOf` in apps/web/src/theme/spec.ts). */
function colorOf(theme: string, accent: string | null, token: string): string {
  const t = SPEC.themes.find(({ id }) => id === theme)!;
  const a = accent ? SPEC.accents.find(({ id }) => id === accent) : undefined;
  return (t.follows.includes(token) && a?.[t.kind][token]) || t.colors[token]!;
}

const PICKS: [ThemeId, AccentId | null][] = THEMES.flatMap((theme) => [null, ...ACCENTS].map((accent): [ThemeId, AccentId | null] => [theme, accent]));

/** A stylesheet's text without its comments. */
const css = (path: string) => readFileSync(join(HERE, "../../src/ui", path), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

/** The custom properties in a stylesheet's `:root` block. */
function rootVariables(text: string): Record<string, string> {
  const body = /:root\s*\{([^}]*)\}/.exec(text)![1]!;
  return Object.fromEntries([...body.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map(([, name, value]) => [name!, value!.trim()]));
}

describe("the themes", () => {
  it("are the spec's, in its order, as are the accents", () => {
    expect([...THEMES]).toEqual(SPEC.themes.map(({ id }) => id));
    expect([...ACCENTS]).toEqual(SPEC.accents.map(({ id }) => id));
  });

  it("give every color the app draws a token of the spec, and draw every token", () => {
    const tokens = Object.keys(SPEC.themes[0]!.colors).sort();
    for (const theme of SPEC.themes) expect(Object.keys(theme.colors).sort(), theme.id).toEqual(tokens);
    // The shade is drawn through its strengths (--shade-60), not as a color of its own.
    expect([...new Set(ROLES.map(([, token]) => token)), "shade"].sort()).toEqual(tokens);
  });

  it("draw each theme under each accent in the spec's colors", () => {
    for (const [theme, accent] of PICKS) {
      const vars = themeVariables(theme, accent);
      for (const [name, token] of ROLES) expect(vars[`--${name}`], `${theme} ${accent ?? "own"} --${name}`).toBe(colorOf(theme, accent, token));
      for (const [name, value] of Object.entries(vars)) {
        if (!name.endsWith("-rgb")) continue;
        expect(value, `${theme} ${accent ?? "own"} ${name}`).toBe(channels(vars[name.slice(0, -4)]!).slice(0, 3).join(", "));
      }
      const [r, g, b, a] = channels(colorOf(theme, accent, "shade"));
      expect(vars["--shade-60"]).toBe(`rgba(${r}, ${g}, ${b}, ${Math.round(a * 600) / 1000})`);
      expect(vars["--on-art"]).toBe(`rgba(${channels(colorOf(theme, accent, "bg")).slice(0, 3).join(", ")}, 0.8)`);
    }
  });

  it("draw the players of a light theme in its dark twin, under the same accent", () => {
    for (const [theme, accent] of PICKS) {
      const spec = SPEC.themes.find(({ id }) => id === theme)!;
      const blocks = themeCss(theme, accent).split("\n");
      if (spec.kind === "dark") {
        expect(blocks, theme).toHaveLength(1);
        continue;
      }
      expect(blocks[1], theme).toMatch(/^\[data-dark\]\{/);
      for (const [name, token] of ROLES) expect(blocks[1]).toContain(`--${name}:${colorOf(spec.twin!, accent, token)};`);
    }
  });

  it("start as Midnight in the stylesheet, before the picked one is drawn", () => {
    const midnight = themeVariables("midnight", null);
    const root = rootVariables(css("theme.css"));
    for (const [name, value] of Object.entries(midnight)) expect(root[name], name).toBe(value);
  });

  it("are the only colors the stylesheets draw, but for black and white on pictures and video", () => {
    for (const file of ["theme.css", "screens.css"]) {
      const text = css(file).replace(/:root\s*\{[^}]*\}/, "");
      const colors = [...text.matchAll(/#[0-9a-f]{3,8}\b|rgba?\(\s*\d[^)]*\)/gi)].map(([c]) => c!.toLowerCase().replace(/\s/g, ""));
      const others = colors.filter((c) => !/^(#000|#fff|#000000|#ffffff|rgba\(0,0,0,[\d.]+\))$/.test(c));
      expect(others, file).toEqual([]);
    }
  });

  it("draw Sunshine's rainbow down the open rail as the web draws its sidebar", () => {
    const web = readFileSync(join(REPO, "apps/web/src/index.css"), "utf8");
    const block = /\[data-theme="sunshine"\]\s*\{([\s\S]*?)\n\}/.exec(web)![1]!;
    const sidebar = /--sidebar:\s*([\s\S]*?);/.exec(block)![1]!;
    const plain = (s: string) =>
      s
        .replace(/rgb\((\d+) (\d+) (\d+) \/ ([\d.]+)\)/g, "rgba($1, $2, $3, $4)")
        .replace(/\s+/g, " ")
        .replace(/\( /g, "(")
        .replace(/ \)/g, ")")
        .trim();
    expect(plain(SUNSHINE_SIDEBAR)).toBe(plain(sidebar));
    expect(themeVariables("sunshine", "mint")["--sidebar"]).toBe(SUNSHINE_SIDEBAR);
    for (const theme of THEMES.filter((t) => t !== "sunshine")) expect(themeVariables(theme, null)["--sidebar"]).toBe("none");
  });

  it("know Midnight and the theme's own accent for ids they don't", () => {
    expect(themeId("dusk")).toBe("midnight");
    expect(themeId(null)).toBe("midnight");
    expect(themeId("starlight")).toBe("starlight");
    expect(accentId("fire")).toBeNull();
    expect(accentId("mint")).toBe("mint");
  });

  it("are drawn as one stylesheet, written again on a pick", () => {
    applyTheme("sunshine", "gold");
    applyTheme("blacklight", "miami");
    const sheets = document.querySelectorAll("style#pley-theme");
    expect(sheets).toHaveLength(1);
    expect(sheets[0]!.textContent).toBe(themeCss("blacklight", "miami"));
    expect(document.documentElement.getAttribute("data-theme")).toBe("blacklight");
  });
});

function memoryStore(): KeyValue & { kept: Map<string, string> } {
  const kept = new Map<string, string>();
  return { kept, get: (k) => kept.get(k) ?? null, set: (k, v) => void kept.set(k, v), remove: (k) => void kept.delete(k) };
}

describe("a TV's theme", () => {
  const app = (store: KeyValue) => new AppState({ name: "tizen", clientId: "test", storage: store } as unknown as Platform, () => Promise.reject(new Error("no network in tests")));

  it("is Midnight with its own accent on a fresh install", () => {
    const state = app(memoryStore());
    expect(state.theme.value).toBe("midnight");
    expect(state.accent.value).toBeNull();
  });

  it("is kept on the TV by its ids, the accent whatever the theme", () => {
    const store = memoryStore();
    const state = app(store);
    state.chooseTheme("greenlight");
    state.chooseAccent("amber");
    state.chooseTheme("twilight");
    expect(store.kept.get("pley.theme")).toBe("twilight");
    expect(store.kept.get("pley.accent")).toBe("amber");
    const again = app(store);
    expect(again.theme.value).toBe("twilight");
    expect(again.accent.value).toBe("amber");
    again.chooseAccent(null);
    expect(store.kept.has("pley.accent")).toBe(false);
  });

  it("shows Midnight and the theme's own accent for ids from another build", () => {
    const store = memoryStore();
    store.set("pley.theme", "aurora");
    store.set("pley.accent", "sunset");
    const state = app(store);
    expect(state.theme.value).toBe("midnight");
    expect(state.accent.value).toBeNull();
  });
});
