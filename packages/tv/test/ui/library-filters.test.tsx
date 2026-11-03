import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ItemSummary, Library, LibraryFilters, User } from "../../src/api/types";
import type { KeyValue } from "../../src/auth/tokens";
import type { Platform } from "../../src/platform";
import { AppState } from "../../src/state";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import type { Key } from "../../src/ui/keys";
import { AppContext } from "../../src/ui/screens/common";
import { LibraryScreen } from "../../src/ui/screens/library";
import { ParamsStore } from "../../src/ui/screens/library-data";
import type { Entry } from "../../src/ui/shell/navigator";

/** The Library tab's resolution and dynamic range filters, and the posters' quality badges they bring. */

function memoryStore(): KeyValue {
  const kept = new Map<string, string>();
  return {
    get: (k) => kept.get(k) ?? null,
    set: (k, v) => void kept.set(k, v),
    remove: (k) => void kept.delete(k),
  };
}

const MOVIES = { id: 1, name: "Movies", kind: "movies", item_count: 3, scanning: false } as Library;
const SERVER = "http://pley.test";

const item = (id: number): ItemSummary =>
  ({
    id,
    kind: "movie",
    title: `Film ${id}`,
    user_state: { watched: false, position_ms: 0, play_count: 0 },
    media: { bitrate: 42_000_000, size: 25_000_000_000, width: 3840, height: 2160, video_codec: "hevc", dynamic_range: "dolby_vision" },
  }) as ItemSummary;

let detach: (() => void) | null = null;
let root: HTMLElement | null = null;

function setup(filters: LibraryFilters, store: KeyValue = memoryStore()) {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const platform = { name: "tizen", clientId: "test", storage: store } as unknown as Platform;
  const app = new AppState(platform, () => Promise.reject(new Error("no network in tests")));
  const asked: Record<string, unknown>[] = [];
  const api = {
    server: SERVER,
    // The whole library unfiltered; two of its three titles for any filter.
    libraryItems: (_id: number, q: Record<string, unknown>) => {
      asked.push(q);
      const filtered = ["watched", "genre", "year", "resolution", "dynamic_range"].some((k) => q[k] !== undefined);
      return Promise.resolve(filtered ? { total: 2, items: [item(1), item(2)] } : { total: 3, items: [item(1), item(2), item(3)] });
    },
    libraryFilters: () => Promise.resolve(filters),
    libraryHubs: () => Promise.resolve({ hubs: [] }),
    collections: () => Promise.resolve({ collections: [] }),
    prefetch: () => Promise.resolve(),
    item: () => Promise.reject(new Error("not here")),
  };
  (app.api as { value: unknown }).value = api;
  app.server.value = SERVER;
  app.me.value = { id: 1 } as User;
  app.libraries.value = [MOVIES];
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  detach = input.attach(document);
  const images = { url: (u: string) => u, ready: () => null, isMissing: () => false, is: () => false, load: () => () => undefined };
  const opened: unknown[] = [];
  const ui = {
    focus,
    input,
    nav: { open: (key: unknown) => void opened.push(key), pop: () => true },
    api,
    images,
    toast: () => undefined,
    claim: (el: HTMLElement, _fid: string | null, initial: boolean) => {
      const current = focus.current;
      if (initial && focus.inScope(el) && !(current && current.isConnected && focus.inScope(current))) focus.focus(el);
    },
    toRail: () => false,
    scale: 1,
    tier: "high",
    reducedMotion: false,
  } as unknown as Ui;
  const entry: Entry = { key: { name: "library", id: 1, tab: "library" }, id: 1, memory: null, state: {} };
  act(() =>
    render(
      <AppContext.Provider value={app}>
        <UiContext.Provider value={ui}>
          <LibraryScreen entry={entry} id={1} tab="library" />
        </UiContext.Provider>
      </AppContext.Provider>,
      root!,
    ),
  );
  const settle = () =>
    act(async () => {
      for (let i = 0; i < 10; i++) await Promise.resolve();
    });
  const press = (el: HTMLElement) => {
    act(() => void focus.focus(el));
    act(() => input.dispatch("ok", "down", false));
    act(() => input.dispatch("ok", "up", false));
  };
  const chip = (fid: string) => root!.querySelector<HTMLElement>(`[data-fid="${fid}"]`);
  /** The side panel's rows, by what they say. */
  const rows = () => Array.from(root!.querySelectorAll<HTMLElement>(".side-panel .panel-row")).map((r) => r.querySelector(".t-body")?.textContent ?? "");
  const row = (label: string) => {
    const found = Array.from(root!.querySelectorAll<HTMLElement>(".side-panel .panel-row")).filter((r) => r.querySelector(".t-body")?.textContent === label)[0];
    if (!found) throw new Error(`no row ${label} in ${rows().join(", ")}`);
    return found;
  };
  const badges = () => Array.from(root!.querySelectorAll(".tile-badge")).map((b) => b.textContent);
  /** The count after the chips. */
  const count = () => root!.querySelector(".chips-count")?.textContent ?? null;
  /** What has focus: a chip's id, a panel row's label; null when nothing on the page has it. */
  const focused = () => {
    const el = focus.current;
    if (!el || !el.isConnected) return null;
    return el.getAttribute("data-fid") ?? el.getAttribute("aria-label");
  };
  /** Whether a panel's row has the check of the choice in force. */
  const checked = (label: string) => !!row(label).querySelector(".check");
  /** A key pressed and let go where focus is. */
  const key = (k: Key) => {
    act(() => input.dispatch(k, "down", false));
    act(() => input.dispatch(k, "up", false));
  };
  /** A closed panel gives focus back a moment after it goes. */
  const later = () => act(() => void vi.advanceTimersByTime(20));
  return { asked, settle, press, chip, rows, row, badges, count, focused, checked, key, later, store, opened };
}

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root = null;
  vi.useRealTimers();
});

const QUALITY: LibraryFilters = { genres: ["Drama"], years: [1999], resolutions: ["4k", "1080"], dynamic_ranges: ["hdr", "dolby_vision", "sdr"] };

describe("a library's resolution and dynamic range", () => {
  it("are chosen in the Filter panel, after Year, by their words", async () => {
    const t = setup(QUALITY);
    await t.settle();
    expect(t.chip("chip/resolution")).toBeNull();
    expect(t.count()).toBe("3 movies");
    t.press(t.chip("chip/filter")!);
    expect(t.rows()).toEqual(["All", "Unwatched", "Watched", "Genre", "Year", "Resolution", "Dynamic range"]);
    expect(t.row("Resolution").textContent).toContain("All resolutions");
    t.press(t.row("Resolution"));
    expect(t.rows()).toEqual(["All resolutions", "4K", "1080p"]);
    t.press(t.row("4K"));
    await t.settle();
    expect(t.asked.at(-1)).toMatchObject({ resolution: "4k", dynamic_range: undefined });
    // How many of the library's titles are left.
    expect(t.count()).toBe("2 of 3 movies");
    // A chip while chosen, saying it; it opens the list again.
    expect(t.chip("chip/resolution")!.textContent).toBe("4K");
    t.press(t.chip("chip/resolution")!);
    expect(t.rows()).toEqual(["All resolutions", "4K", "1080p"]);
    t.press(t.row("All resolutions"));
    await t.settle();
    expect(t.chip("chip/resolution")).toBeNull();
    expect(t.count()).toBe("3 movies");

    t.press(t.chip("chip/filter")!);
    t.press(t.row("Dynamic range"));
    expect(t.rows()).toEqual(["All dynamic ranges", "Any HDR", "Dolby Vision", "SDR"]);
    t.press(t.row("Dolby Vision"));
    await t.settle();
    expect(t.asked.at(-1)).toMatchObject({ resolution: undefined, dynamic_range: "dolby_vision" });
    expect(t.chip("chip/dynamicRange")!.textContent).toBe("Dolby Vision");
    expect(t.count()).toBe("2 of 3 movies");
    // Kept with the library's view.
    expect(new ParamsStore(t.store, SERVER, 1).get(1, "movies").dynamicRange).toBe("dolby_vision");
  });

  it("count as filters: Clear filters clears them, and Save as… carries them", async () => {
    const store = memoryStore();
    new ParamsStore(store, SERVER, 1).set(1, { sort: "title", order: "asc", watched: null, genre: "Drama", year: null, resolution: "4k", dynamicRange: "hdr" });
    const t = setup(QUALITY, store);
    await t.settle();
    expect(t.asked[0]).toMatchObject({ genre: "Drama", resolution: "4k", dynamic_range: "hdr" });
    expect(t.chip("chip/resolution")!.textContent).toBe("4K");
    expect(t.chip("chip/dynamicRange")!.textContent).toBe("Any HDR");
    expect(t.count()).toBe("2 of 3 movies");
    t.press(t.chip("chip/filter")!);
    expect(t.rows().slice(-2)).toEqual(["Clear filters", "Save as…"]);
    t.press(t.row("Save as…"));
    expect(t.opened.at(-1)).toEqual({ name: "newPlaylist", smart: { libraryId: 1, filter: "genre=Drama&resolution=4k&dynamicRange=hdr&sort=titleSort" } });
    t.press(t.chip("chip/filter")!);
    t.press(t.row("Clear filters"));
    await t.settle();
    expect(t.asked.at(-1)).toMatchObject({ genre: undefined, resolution: undefined, dynamic_range: undefined });
    expect(t.chip("chip/resolution")).toBeNull();
    expect(t.chip("chip/dynamicRange")).toBeNull();
    expect(t.count()).toBe("3 movies");
  });

  it("cleared from their own chip, which goes with them, leave focus on Filter", async () => {
    const t = setup(QUALITY);
    await t.settle();
    t.press(t.chip("chip/filter")!);
    t.press(t.row("Resolution"));
    t.press(t.row("4K"));
    await t.settle();
    t.later();
    // Back from the chip's list: the chip again.
    t.press(t.chip("chip/resolution")!);
    expect(t.focused()).toBe("4K");
    t.key("back");
    t.later();
    expect(t.focused()).toBe("chip/resolution");
    t.press(t.chip("chip/resolution")!);
    t.press(t.row("All resolutions"));
    await t.settle();
    t.later();
    expect(t.chip("chip/resolution")).toBeNull();
    expect(t.focused()).toBe("chip/filter");
    // The next key goes on along the chips, not to the tabs above.
    t.key("right");
    expect(t.focused()).toBe("chip/unwatched");

    t.press(t.chip("chip/filter")!);
    t.press(t.row("Dynamic range"));
    t.press(t.row("SDR"));
    await t.settle();
    t.press(t.chip("chip/dynamicRange")!);
    t.press(t.row("All dynamic ranges"));
    await t.settle();
    t.later();
    expect(t.chip("chip/dynamicRange")).toBeNull();
    expect(t.focused()).toBe("chip/filter");
  });

  it("keep one chosen that the server no longer offers in its list, checked and focused", async () => {
    const store = memoryStore();
    new ParamsStore(store, SERVER, 1).set(1, { sort: "title", order: "asc", watched: null, genre: null, year: null, resolution: "sd", dynamicRange: "hlg" });
    const t = setup({ genres: [], years: [], resolutions: [], dynamic_ranges: ["hdr", "sdr"] }, store);
    await t.settle();
    t.press(t.chip("chip/resolution")!);
    expect(t.rows()).toEqual(["All resolutions", "SD"]);
    expect(t.checked("SD")).toBe(true);
    expect(t.focused()).toBe("SD");
    // OK keeps it, and the panel goes, focus back on its chip.
    t.key("ok");
    await t.settle();
    t.later();
    expect(t.rows()).toEqual([]);
    expect(t.asked.at(-1)).toMatchObject({ resolution: "sd", dynamic_range: "hlg" });
    expect(t.focused()).toBe("chip/resolution");
    // After those the server offers; through the Filter panel too.
    t.press(t.chip("chip/filter")!);
    t.press(t.row("Dynamic range"));
    expect(t.rows()).toEqual(["All dynamic ranges", "Any HDR", "SDR", "HLG"]);
    expect(t.checked("HLG")).toBe(true);
    expect(t.focused()).toBe("HLG");
  });

  it("aren't offered where the server lists none, unless one is chosen", async () => {
    const t = setup({ genres: ["Drama"], years: [1999] });
    await t.settle();
    t.press(t.chip("chip/filter")!);
    expect(t.rows()).toEqual(["All", "Unwatched", "Watched", "Genre", "Year"]);

    const store = memoryStore();
    new ParamsStore(store, SERVER, 1).set(1, { sort: "title", order: "asc", watched: null, genre: null, year: null, resolution: "sd", dynamicRange: null });
    const chosen = setup({ genres: [], years: [], resolutions: [], dynamic_ranges: [] }, store);
    await chosen.settle();
    chosen.press(chosen.chip("chip/filter")!);
    expect(chosen.rows()).toEqual(["All", "Unwatched", "Watched", "Genre", "Year", "Resolution", "Clear filters", "Save as…"]);
  });
});

describe("the posters' quality badges", () => {
  it("show only sorted by bitrate or filtered by resolution or dynamic range", async () => {
    const t = setup(QUALITY);
    await t.settle();
    expect(t.badges()).toEqual([]);
    t.press(t.chip("chip/filter")!);
    t.press(t.row("Resolution"));
    t.press(t.row("4K"));
    await t.settle();
    expect(t.badges()).toEqual(["4K", "DV", "4K", "DV"]);

    const store = memoryStore();
    new ParamsStore(store, SERVER, 1).set(1, { sort: "bitrate", order: "desc", watched: null, genre: null, year: null, resolution: null, dynamicRange: null });
    const byBitrate = setup(QUALITY, store);
    await byBitrate.settle();
    expect(byBitrate.badges()).toEqual(["4K", "DV", "4K", "DV", "4K", "DV"]);

    const filtered = memoryStore();
    new ParamsStore(filtered, SERVER, 1).set(1, { sort: "title", order: "asc", watched: false, genre: "Drama", year: 1999, resolution: null, dynamicRange: null });
    const byGenre = setup(QUALITY, filtered);
    await byGenre.settle();
    expect(byGenre.badges()).toEqual([]);
  });
});
