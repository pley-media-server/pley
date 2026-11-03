import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import type { Library, LibraryKind, RequestSummary } from "../../src/api/types";
import { currentEntry, RAIL_ENTRY, RAIL_FIRST, RAIL_GAP, RAIL_SETTINGS, railEntries, railScroll, requestsEntry } from "../../src/ui/shell/rail";
import fixture from "../fixtures/rail.json";

/** The Android app's ShellRulesTest, for the TVs' rail. */
const libraries = (n: number): Library[] =>
  Array.from({ length: n }, (_, i) => ({ id: i + 1, name: `Library ${i + 1}`, kind: (i + 1) % 2 === 0 ? "shows" : "movies" }) as Library);
const ids = (n: number, nowPlaying = false) => railEntries(libraries(n), nowPlaying).top.map((e) => e.id);

describe("the rail", () => {
  it("lists up to six libraries, then Collections", () => {
    expect(ids(3)).toEqual(["search", "home", "library:1", "library:2", "library:3", "playlists", "collections"]);
    expect(railEntries(libraries(3)).top.at(-1)!.key).toEqual({ name: "collections", libraryId: 1 });
    expect(ids(6)).toEqual(["search", "home", "library:1", "library:2", "library:3", "library:4", "library:5", "library:6", "playlists", "collections"]);
  });

  it("puts Libraries in Collections' place with more than six", () => {
    expect(ids(7)).toEqual(["search", "home", "library:1", "library:2", "library:3", "library:4", "library:5", "library:6", "libraries", "playlists"]);
  });

  it("has no Collections without libraries", () => {
    expect(ids(0)).toEqual(["search", "home", "playlists"]);
  });

  it("puts Now Playing last while music has a queue, leaving nothing out for it", () => {
    expect(ids(3, true)).toEqual(["search", "home", "library:1", "library:2", "library:3", "playlists", "collections", "now-playing"]);
    expect(ids(8, true)).toEqual([
      "search",
      "home",
      "library:1",
      "library:2",
      "library:3",
      "library:4",
      "library:5",
      "library:6",
      "libraries",
      "playlists",
      "now-playing",
    ]);
  });

  it("puts Live TV after Home where the server has channels, leaving nothing out for it", () => {
    const live = (n: number, nowPlaying = false) => railEntries(libraries(n), nowPlaying, true).top.map((e) => e.id);
    expect(live(2)).toEqual(["search", "home", "live", "library:1", "library:2", "playlists", "collections"]);
    expect(live(4, true)).toEqual(["search", "home", "live", "library:1", "library:2", "library:3", "library:4", "playlists", "collections", "now-playing"]);
    expect(live(0)).toEqual(["search", "home", "live", "playlists"]);
    expect(railEntries(libraries(2), false, true).top[2]!.key).toEqual({ name: "live" });
    expect(currentEntry({ name: "live" }, railEntries(libraries(2), false, true))).toBe("live");
  });

  it("marks the entry that opened the stack's first page", () => {
    const rail = railEntries(libraries(8), true);
    expect(currentEntry({ name: "nowPlaying" }, rail)).toBe("now-playing");
    expect(currentEntry({ name: "library", id: 6 }, rail)).toBe("library:6");
    // Past the sixth: Libraries lists it.
    expect(currentEntry({ name: "library", id: 7 }, rail)).toBe("libraries");
    expect(currentEntry({ name: "search", query: "blade" }, rail)).toBe("search");
    expect(currentEntry({ name: "profiles" }, rail)).toBe("profile");
    expect(currentEntry({ name: "settings" }, rail)).toBe("settings");
    // Collections without its own entry: the library's.
    expect(currentEntry({ name: "collections", libraryId: 1 }, rail)).toBe("library:1");
    expect(currentEntry({ name: "collections", libraryId: 1 }, railEntries(libraries(3)))).toBe("collections");
    expect(currentEntry({ name: "nowPlaying" }, railEntries(libraries(3)))).toBeNull();
    expect(currentEntry(null, rail)).toBeNull();
  });
});

describe("the rail, as the shared fixture has it", () => {
  const ofKinds = (kinds: string[]): Library[] =>
    kinds.map((kind, i) => ({ id: i + 1, name: `Library ${i + 1}`, kind: kind as LibraryKind }) as Library);

  it("entries", () => {
    for (const c of fixture.entries) {
      const top = railEntries(ofKinds(c.in.libraries), c.in.nowPlaying, c.in.live).top.map((e) => e.id);
      expect([c.in, top]).toEqual([c.in, c.out]);
    }
  });

  it("scroll", () => {
    for (const c of fixture.scroll) {
      // With an entry focused, where it was scrolled before makes no difference.
      const from = c.in.scroll === undefined ? [0, 37, 1000] : [c.in.scroll];
      for (const s of from) expect([c.in, s, railScroll(s, c.in.focused, c.in.count)]).toEqual([c.in, s, c.out]);
    }
  });

  it("geometry: theme.css lays entries out 40 tall and 44 apart from y 67, under the avatar, nine of them above Settings at y 473", () => {
    const canvas = px(css("#app").height);
    const items = css(".rail-items");
    const view = css(".rail-view");
    const entry = px(css(".rail-item").height);
    const gap = px(css(".rail-scroll > .rail-item")["margin-bottom"]);
    const avatar = px(css(".rail-item.avatar").height);
    const viewPadTop = px(view.padding?.split(" ")[0]);
    expect([entry, entry + gap, avatar]).toEqual([40, 44, 48]);
    // The view starts at the avatar's foot, its first entry 4 below; it ends where Settings, last in the column, starts.
    expect(px(view.top)).toBe(avatar);
    const first = px(items.top) + avatar + viewPadTop;
    const settings = canvas - px(items.bottom) - entry;
    expect([first, settings, canvas - px(items.bottom) - px(view.bottom)]).toEqual([67, 473, 473]);
    // What railScroll reckons with is what the stylesheet draws.
    expect([RAIL_ENTRY, RAIL_GAP, RAIL_FIRST, RAIL_SETTINGS]).toEqual([entry, gap, first, settings]);
    const fits = (n: number) => first + n * (entry + gap) <= settings;
    expect([fits(9), fits(10)]).toEqual([true, false]);
  });
});

/** theme.css's declarations for `selector`, from every rule that names it (alone or in a list). */
function css(selector: string): Record<string, string | undefined> {
  const text = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../../src/ui/theme.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const out: Record<string, string> = {};
  for (const m of text.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (!m[1]!.split(",").some((s) => s.trim() === selector)) continue;
    for (const d of m[2]!.split(";")) {
      const at = d.indexOf(":");
      if (at > 0) out[d.slice(0, at).trim()] = d.slice(at + 1).trim();
    }
  }
  return out;
}

/** "40px" → 40; anything else fails the test. */
function px(v: string | undefined): number {
  const m = /^(-?\d+(?:\.\d+)?)px$/.exec(v ?? "");
  if (!m) throw new Error(`not a length in px: ${v}`);
  return Number(m[1]);
}

describe("the rail's way to requests", () => {
  const summary = (over: Partial<RequestSummary> = {}): RequestSummary => ({
    enabled: true,
    access: "ask",
    movies_left: null,
    seasons_left: null,
    movies_limit: null,
    seasons_limit: null,
    movies_next_at: null,
    seasons_next_at: null,
    manage: false,
    pending: 0,
    failed: 0,
    unseen: 0,
    own: 0,
    changed_at: null,
    sign_in_again: null,
    ...over,
  });
  const withRequests = (n: number, s: RequestSummary, nowPlaying = false) => railEntries(libraries(n), nowPlaying, false, requestsEntry(s)).top.map((e) => e.id);

  it("is Discover after Home where the profile may ask, else its requests, else nothing", () => {
    expect(requestsEntry(summary())).toMatchObject({ id: "discover", label: "Discover", key: { name: "discover" } });
    expect(requestsEntry(summary({ access: "off", own: 2 }))).toMatchObject({ id: "requests", label: "Your requests", key: { name: "requests" } });
    expect(requestsEntry(summary({ access: "off", own: 0 }))).toBeNull();
    expect(requestsEntry(summary({ enabled: false }))).toBeNull();
    expect(requestsEntry(null)).toBeNull();
    expect(withRequests(2, summary())).toEqual(["search", "home", "discover", "library:1", "library:2", "playlists", "collections"]);
  });

  it("carries the one number: managers' waiting requests in the danger colour while some failed, everyone else's news", () => {
    expect(requestsEntry(summary({ manage: true, pending: 2, failed: 1, unseen: 1 }))?.badge).toEqual({ count: 4, danger: true });
    expect(requestsEntry(summary({ unseen: 2, pending: 9 }))?.badge).toEqual({ count: 2, danger: false });
    expect(requestsEntry(summary())?.badge).toBeUndefined();
  });

  it("comes after Home, leaving nothing out for it", () => {
    expect(withRequests(3, summary())).toEqual(["search", "home", "discover", "library:1", "library:2", "library:3", "playlists", "collections"]);
    expect(withRequests(7, summary(), true)).toEqual([
      "search",
      "home",
      "discover",
      "library:1",
      "library:2",
      "library:3",
      "library:4",
      "library:5",
      "library:6",
      "libraries",
      "playlists",
      "now-playing",
    ]);
  });

  it("is the current entry for Discover's pages and the requests reached from it", () => {
    const rail = railEntries(libraries(2), false, false, requestsEntry(summary()));
    expect(currentEntry({ name: "discover" }, rail)).toBe("discover");
    expect(currentEntry({ name: "title", kind: "movie", tmdbId: 1 }, rail)).toBe("discover");
    expect(currentEntry({ name: "requests" }, rail)).toBe("discover");
    const own = railEntries(libraries(2), false, false, requestsEntry(summary({ access: "off", own: 1 })));
    expect(currentEntry({ name: "requests" }, own)).toBe("requests");
  });
});
