import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import type { User } from "../../src/api/types";
import type { KeyValue } from "../../src/auth/tokens";
import type { Platform } from "../../src/platform";
import { AppState } from "../../src/state";
import {
  accentId,
  ACCENTS,
  applyTheme,
  channels,
  ROLES,
  shownPick,
  SUNSHINE_SIDEBAR,
  themeCss,
  themeId,
  THEMES,
  themeVariables,
  type AccentId,
  type ProfileOf,
  type ThemeId,
} from "../../src/ui/themes";
import fixture from "../fixtures/appearance.json";
import { outsideProfile, type PageKey } from "../../src/ui/shell/navigator";

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

function memoryStore(kept: Record<string, string> = {}): KeyValue & { kept: Map<string, string> } {
  const map = new Map(Object.entries(kept));
  return { kept: map, get: (k) => map.get(k) ?? null, set: (k, v) => void map.set(k, v), remove: (k) => void map.delete(k) };
}

describe("the pick drawn (fixtures/appearance.json)", () => {
  it("is Midnight outside a profile, and the profile's own inside it once known", () => {
    for (const c of fixture.shown) {
      const kept = new Map(Object.entries(c.kept as unknown as Record<string, string>));
      expect(shownPick((k) => kept.get(k) ?? null, c.profile as ProfileOf | null, c.outside), c.name).toEqual(c.out);
    }
  });

  it("knows the screens outside a profile", () => {
    for (const c of fixture.screens) expect(outsideProfile({ name: c.screen } as PageKey), c.name).toBe(c.outside);
  });

  it("is kept for the profile alone", () => {
    for (const c of fixture.picks) {
      const store = memoryStore(c.kept as unknown as Record<string, string>);
      const state = app(store);
      state.server.value = c.profile.server;
      state.me.value = { id: c.profile.id } as User;
      state.chooseTheme(c.pick.theme as ThemeId);
      state.chooseAccent(c.pick.accent as AccentId | null);
      expect(Object.fromEntries(store.kept), c.name).toEqual(c.out);
    }
  });
});

const app = (store: KeyValue) => new AppState({ name: "tizen", clientId: "test", storage: store } as unknown as Platform, () => Promise.reject(new Error("no network in tests")));

const SERVER = "http://pley.local:7539";

/** A TV signed in to `server`, which says `who()` is watching; what else it asks never answers. */
function signedIn(store: KeyValue, who: () => number, server = SERVER) {
  const state = app(store);
  state.server.value = server;
  const quiet = () => new Promise(() => undefined);
  (state.api as { value: unknown }).value = new Proxy(
    {},
    {
      get: (_, name) =>
        // who() 0: the server doesn't answer who is watching.
        name === "me"
          ? () => (who() > 0 ? Promise.resolve({ id: who(), name: `p${who()}` }) : Promise.reject(new Error("offline")))
          : name === "libraries"
            ? () => Promise.resolve([])
            : name === "profiles"
              ? () => Promise.resolve([])
              : quiet,
    },
  );
  return state;
}

const drawn = (state: AppState) => {
  const pick = state.drawn();
  return [pick.theme, pick.accent];
};

describe("a TV's theme", () => {
  it("is Midnight with its own accent on a fresh install", () => {
    const state = app(memoryStore());
    expect(drawn(state)).toEqual(["midnight", null]);
  });

  it("is each profile's own: two profiles keep different picks, and a switch shows the other's at once", async () => {
    const store = memoryStore();
    let profile = 1;
    const state = signedIn(store, () => profile);
    await state.loadAccount();
    state.chooseTheme("greenlight");
    state.chooseAccent("amber");
    profile = 2;
    await state.switched();
    // New to the TV: Midnight in its own accent, not the profile before's.
    expect(drawn(state)).toEqual(["midnight", null]);
    state.chooseTheme("blacklight");
    state.chooseAccent("miami");
    profile = 1;
    await state.switched();
    expect(drawn(state)).toEqual(["greenlight", "amber"]);
    profile = 2;
    await state.switched();
    expect(drawn(state)).toEqual(["blacklight", "miami"]);
  });

  it("draws Midnight outside a profile, though the profile shown before picked another, and its pick back inside", async () => {
    const state = signedIn(memoryStore(), () => 5);
    await state.loadAccount();
    state.chooseTheme("starlight");
    state.chooseAccent("gold");
    // "Who's watching?", say.
    state.outside.value = true;
    expect(drawn(state)).toEqual(["midnight", null]);
    // Back out of it.
    state.outside.value = false;
    expect(drawn(state)).toEqual(["starlight", "gold"]);
  });

  it("starts in the saved signed-in profile's pick from the first frame", () => {
    const store = memoryStore({
      "pley.server": SERVER,
      "pley.me": JSON.stringify({ id: 3, name: "Sam" }),
      [`pley.theme.${SERVER}.3`]: "twilight",
      [`pley.accent.${SERVER}.3`]: "mint",
    });
    expect(drawn(app(store))).toEqual(["twilight", "mint"]);
    // With no profile saved, Midnight.
    store.remove("pley.me");
    expect(drawn(app(store))).toEqual(["midnight", null]);
  });

  it("keeps a pick for each server: the same user id on two servers is two profiles", async () => {
    const store = memoryStore();
    const den = "http://den.local:7539";
    const here = signedIn(store, () => 7);
    await here.loadAccount();
    here.chooseTheme("greenlight");
    const there = signedIn(store, () => 7, den);
    await there.loadAccount();
    expect(drawn(there)).toEqual(["midnight", null]);
    there.chooseTheme("blacklight");
    there.chooseAccent("gold");
    const back = signedIn(store, () => 7);
    await back.loadAccount();
    expect(drawn(back)).toEqual(["greenlight", null]);
    expect(store.kept.get(`pley.theme.${den}.7`)).toBe("blacklight");
    expect(store.kept.get(`pley.theme.${SERVER}.7`)).toBe("greenlight");
  });

  it("starts in Midnight after signing out, and keeps the profile's pick for its next sign-in", async () => {
    const store = memoryStore();
    const state = signedIn(store, () => 4);
    await state.loadAccount();
    state.chooseTheme("sunshine");
    await state.signOut();
    expect(drawn(app(store))).toEqual(["midnight", null]);
    const again = signedIn(store, () => 4);
    await again.loadAccount();
    expect(drawn(again)).toEqual(["sunshine", null]);
  });

  it("doesn't keep the profile before's theme while a switch can't say who is watching", async () => {
    const store = memoryStore();
    let profile = 1;
    const state = signedIn(store, () => profile);
    await state.loadAccount();
    state.chooseTheme("starlight");
    // The switch's answer doesn't come.
    profile = 0;
    await state.switched().catch(() => undefined);
    expect(drawn(state)).toEqual(["midnight", null]);
    // A pick meanwhile isn't the profile before's.
    state.chooseTheme("twilight");
    expect(store.kept.get(`pley.theme.${SERVER}.1`)).toBe("starlight");
  });

  it("leaves a server's profile's theme behind when another server is used", async () => {
    const state = signedIn(memoryStore(), () => 6);
    await state.loadAccount();
    state.chooseTheme("greenlight");
    state.use("http://den.local:7539");
    expect(drawn(state)).toEqual(["midnight", null]);
  });

  it("keeps no pick for the profile signed out of", async () => {
    const store = memoryStore();
    const state = signedIn(store, () => 4);
    await state.loadAccount();
    state.chooseTheme("sunshine");
    await state.signOut();
    expect(drawn(state)).toEqual(["midnight", null]);
    state.chooseTheme("blacklight");
    expect(store.kept.get(`pley.theme.${SERVER}.4`)).toBe("sunshine");
  });

  it("shows Midnight and the theme's own accent for ids from another build", () => {
    const store = memoryStore({ "pley.server": SERVER, "pley.me": JSON.stringify({ id: 3, name: "Sam" }), [`pley.theme.${SERVER}.3`]: "aurora", [`pley.accent.${SERVER}.3`]: "sunset" });
    expect(drawn(app(store))).toEqual(["midnight", null]);
  });
});
