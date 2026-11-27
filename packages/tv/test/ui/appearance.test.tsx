import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import type { KeyValue } from "../../src/auth/tokens";
import type { Platform } from "../../src/platform";
import { AppState } from "../../src/state";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import type { Key } from "../../src/ui/keys";
import { AppearanceScreen } from "../../src/ui/screens/appearance";
import { AppContext } from "../../src/ui/screens/common";
import { tokenColor } from "../../src/ui/themes";

/** Settings → Appearance: the nine themes, then the accents, a pick shown at once and kept on the TV. */

let root: HTMLElement | null = null;
let detach: (() => void) | null = null;

function setup(accent = false) {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  // Signed in as profile 1 of the server: its picks are kept for it.
  const kept = new Map<string, string>([
    ["pley.server", "http://pley.local:7539"],
    ["pley.me", JSON.stringify({ id: 1, name: "Alex" })],
  ]);
  const storage: KeyValue = { get: (k) => kept.get(k) ?? null, set: (k, v) => void kept.set(k, v), remove: (k) => void kept.delete(k) };
  const app = new AppState({ name: "tizen", clientId: "test", storage } as unknown as Platform, () => Promise.reject(new Error("no network in tests")));
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  detach = input.attach(document);
  const ui = {
    focus,
    input,
    claim: (el: HTMLElement, _fid: string | null, initial: boolean) => {
      if (initial && focus.inScope(el)) focus.focus(el);
    },
  } as unknown as Ui;
  act(() =>
    render(
      <AppContext.Provider value={app}>
        <UiContext.Provider value={ui}>
          <AppearanceScreen accent={accent} />
        </UiContext.Provider>
      </AppContext.Provider>,
      root!,
    ),
  );
  const press = (key: Key) => {
    act(() => input.dispatch(key, "down", false));
    act(() => input.dispatch(key, "up", false));
  };
  const select = (fid: string) => {
    act(() => void focus.focus(root!.querySelector<HTMLElement>(`[data-fid="${fid}"]`)!));
    press("ok");
  };
  return { root: root!, app, kept, focus, select };
}

afterEach(() => {
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root = null;
});

describe("Appearance", () => {
  it("shows the nine themes in order, each with its name and caption, Midnight ticked", () => {
    const { root } = setup();
    const cards = Array.from(root.querySelectorAll(".theme-card"));
    expect(cards.map((c) => c.querySelector(".theme-name")!.textContent)).toEqual([
      "Midnight",
      "Midday",
      "Twilight",
      "Starlight",
      "Moonlight",
      "Nightshade",
      "Greenlight",
      "Sunshine",
      "Blacklight",
    ]);
    expect(cards[3]!.querySelector(".theme-about")!.textContent).toBe("Champagne, no hangover");
    expect(root.querySelectorAll(".theme-card.current")).toHaveLength(1);
    expect(root.querySelector(".theme-card.current .theme-tick")).not.toBeNull();
    expect(root.querySelector(".theme-card.current")!.getAttribute("data-fid")).toBe("theme-midnight");
  });

  it("draws each preview in its own theme's colors", () => {
    const { root } = setup();
    const preview = root.querySelector<HTMLElement>('[data-fid="theme-sunshine"] .theme-preview')!;
    expect(preview.style.getPropertyValue("--bg")).toBe(tokenColor("sunshine", null, "bg"));
    expect(preview.style.getPropertyValue("--accent")).toBe(tokenColor("sunshine", null, "accent"));
  });

  it("starts on the theme shown, or on the accent with `accent`", () => {
    expect(setup().focus.current?.getAttribute("data-fid")).toBe("theme-midnight");
    expect(setup(true).focus.current?.getAttribute("data-fid")).toBe("accent-own");
  });

  it("offers the theme's own accent first, then the six, each swatch in the color it gives, the one in use ticked", () => {
    const { root } = setup();
    const chips = Array.from(root.querySelectorAll(".accent-chip"));
    expect(chips.map((c) => c.textContent)).toEqual(["Default", "Sky", "Violet", "Miami", "Gold", "Mint", "Amber"]);
    const swatch = (fid: string) => root.querySelector<HTMLElement>(`[data-fid="${fid}"] .accent-swatch`)!;
    expect(swatch("accent-own").classList.contains("picked")).toBe(true);
    expect(swatch("accent-sky").style.background).toBe("rgb(56, 189, 248)");
    expect(swatch("accent-gold").style.background).toBe("rgb(245, 197, 66)");
  });

  it("lays the themes out five to a row, each name and caption under its preview", () => {
    const { root } = setup();
    const grid = root.querySelector('[data-fg="grid"]')!;
    expect(grid.classList.contains("theme-grid")).toBe(true);
    const card = root.querySelector('[data-fid="theme-midnight"]')!;
    expect(Array.from(card.children).map((c) => c.className)).toEqual(["theme-preview", "t-body one-line theme-name", "t-caption lines-2 theme-about", "theme-tick", "theme-ring"]);
  });

  it("shows a pick at once and keeps it on the TV", () => {
    const { root, app, kept, select } = setup();
    select("theme-starlight");
    expect(app.theme.value).toBe("starlight");
    expect(kept.get("pley.theme.http://pley.local:7539.1")).toBe("starlight");
    expect(root.querySelector(".theme-card.current")!.getAttribute("data-fid")).toBe("theme-starlight");
    select("accent-gold");
    expect(app.accent.value).toBe("gold");
    expect(kept.get("pley.accent.http://pley.local:7539.1")).toBe("gold");
    expect(root.querySelector(".accent-chip.current")!.getAttribute("data-fid")).toBe("accent-gold");
    // The other swatches are now the light themes' colors.
    expect(root.querySelector<HTMLElement>('[data-fid="accent-gold"] .accent-swatch')!.classList.contains("picked")).toBe(true);
    expect(root.querySelector<HTMLElement>('[data-fid="accent-own"] .accent-swatch')!.style.background).toBe("rgb(131, 86, 19)");
    select("accent-own");
    expect(app.accent.value).toBeNull();
    expect(kept.has("pley.accent.http://pley.local:7539.1")).toBe(false);
  });
});
