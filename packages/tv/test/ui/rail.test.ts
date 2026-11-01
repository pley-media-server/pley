import { describe, expect, it } from "vitest";
import type { Library, RequestSummary } from "../../src/api/types";
import { currentEntry, railEntries, requestsEntry } from "../../src/ui/shell/rail";

/** The Android app's ShellRulesTest, for the TVs' rail. */
const libraries = (n: number): Library[] =>
  Array.from({ length: n }, (_, i) => ({ id: i + 1, name: `Library ${i + 1}`, kind: (i + 1) % 2 === 0 ? "shows" : "movies" }) as Library);
const ids = (n: number, nowPlaying = false) => railEntries(libraries(n), nowPlaying).top.map((e) => e.id);

describe("the rail", () => {
  it("lists up to three libraries, then Collections", () => {
    expect(ids(3)).toEqual(["search", "home", "library:1", "library:2", "library:3", "playlists", "collections"]);
    expect(railEntries(libraries(3)).top.at(-1)!.key).toEqual({ name: "collections", libraryId: 1 });
  });

  it("puts Libraries in Collections' place with more than three", () => {
    expect(ids(5)).toEqual(["search", "home", "library:1", "library:2", "library:3", "libraries", "playlists"]);
  });

  it("has no Collections without libraries", () => {
    expect(ids(0)).toEqual(["search", "home", "playlists"]);
  });

  it("puts Now Playing last, within seven entries, while music has a queue", () => {
    // In Collections' place.
    expect(ids(3, true)).toEqual(["search", "home", "library:1", "library:2", "library:3", "playlists", "now-playing"]);
    // In the last library's, which Libraries lists.
    expect(ids(5, true)).toEqual(["search", "home", "library:1", "library:2", "libraries", "playlists", "now-playing"]);
    // Added where there is room.
    expect(ids(2, true)).toEqual(["search", "home", "library:1", "library:2", "playlists", "collections", "now-playing"]);
  });

  it("puts Live TV after Home where the server has channels, the libraries making room from the last", () => {
    const live = (n: number, nowPlaying = false) => railEntries(libraries(n), nowPlaying, true).top.map((e) => e.id);
    expect(live(2)).toEqual(["search", "home", "live", "library:1", "library:2", "playlists", "collections"]);
    // Collections goes first; then the last library, which Libraries lists.
    expect(live(3)).toEqual(["search", "home", "live", "library:1", "library:2", "library:3", "playlists"]);
    expect(live(5)).toEqual(["search", "home", "live", "library:1", "library:2", "libraries", "playlists"]);
    expect(live(3, true)).toEqual(["search", "home", "live", "library:1", "libraries", "playlists", "now-playing"]);
    expect(live(0)).toEqual(["search", "home", "live", "playlists"]);
    expect(railEntries(libraries(2), false, true).top[2]!.key).toEqual({ name: "live" });
    expect(currentEntry({ name: "live" }, railEntries(libraries(2), false, true))).toBe("live");
  });

  it("marks the entry that opened the stack's first page", () => {
    const rail = railEntries(libraries(5), true);
    expect(currentEntry({ name: "nowPlaying" }, rail)).toBe("now-playing");
    expect(currentEntry({ name: "library", id: 2 }, rail)).toBe("library:2");
    // Library 3 gave its place to Now Playing: Libraries lists it.
    expect(currentEntry({ name: "library", id: 3 }, rail)).toBe("libraries");
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

  it("makes room by leaving out Collections, then the last library, which Libraries lists", () => {
    expect(withRequests(3, summary())).toEqual(["search", "home", "discover", "library:1", "library:2", "library:3", "playlists"]);
    expect(withRequests(5, summary())).toEqual(["search", "home", "discover", "library:1", "library:2", "libraries", "playlists"]);
    expect(withRequests(3, summary(), true)).toEqual(["search", "home", "discover", "library:1", "libraries", "playlists", "now-playing"]);
    expect(withRequests(5, summary(), true)).toEqual(["search", "home", "discover", "library:1", "libraries", "playlists", "now-playing"]);
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
