import { describe, expect, it } from "vitest";
import type { Credit, Hub, ItemDetail, ItemSummary, Library } from "../../src/api/types";
import { check, connect, hostPort, subnetOf } from "../../src/auth/servers";
import { TokenError, type OAuthError } from "../../src/auth/tokens";
import type { HttpRequest, Transport } from "../../src/net/transport";
import { NetworkError } from "../../src/net/transport";
import { shuffled } from "../../src/player/music";
import { featuredLabel, featuredOf, heroOf, rowKind, seeAllOf, seeAllSort } from "../../src/ui/screens/home";
import { castAndCrew, seasonPosters } from "../../src/ui/screens/item";
import { openingTab } from "../../src/ui/screens/library";
import featured from "../fixtures/featured.json";
import seasonPosterCases from "../fixtures/season-posters.json";
import {
  AlphaIndex,
  cleared,
  countLabel,
  defaultOrder,
  describeSmartFilter,
  fitParams,
  hasFilter,
  letterOf,
  ParamsStore,
  qualityChoices,
  showsBadges,
  showsJump,
  TabStore,
  smartFilterOf,
  sortsFor,
  type LibraryParams,
} from "../../src/ui/screens/library-data";
import { codeHalves, displayCode, typedAddress } from "../../src/ui/screens/onboarding/link";
import { switchRefusal } from "../../src/ui/screens/onboarding/profiles";
import { relativeTime } from "../../src/ui/screens/settings";

/** A movie library's params: the defaults, with `over`. */
const params = (over: Partial<LibraryParams>): LibraryParams => ({ ...fitParams(null, "movies"), ...over });

function summary(over: Partial<ItemSummary>): ItemSummary {
  return { id: 1, library_id: 1, kind: "movie", title: "A", sort_title: "a", added_at: 0, user_state: { position_ms: 0, watched: false, play_count: 0 }, ...over };
}

describe("the A–Z jump", () => {
  const items = ["Alien 1", "Alien 2", "Batman 1", "Ébloui", "Zorro 1", "300", "ünder"].map((t) => ({ title: t, sort_title: t.toLowerCase() }));
  const sorted = items.slice().sort((a, b) => letterOf(a).localeCompare(letterOf(b)));

  it("files titles by their first letter, accents removed, the rest under #", () => {
    expect(letterOf({ title: "Ébloui", sort_title: "" })).toBe("E");
    expect(letterOf({ title: "300", sort_title: "300" })).toBe("#");
    expect(letterOf({ title: "Matrix", sort_title: "matrix" })).toBe("M");
  });

  it("jumps to a letter's first title, else the next letter's, else the one before", () => {
    const alpha = new AlphaIndex(sorted);
    expect(alpha.jumpTarget("A")).toBe(sorted.findIndex((x) => x.title === "Alien 1"));
    // No C: the next letter with titles.
    expect(sorted[alpha.jumpTarget("C")]!.title).toBe("Ébloui");
    // Nothing after Z: the nearest before.
    expect(alpha.jumpTarget("Z")).toBe(sorted.findIndex((x) => x.title === "Zorro 1"));
    expect(alpha.letterAt(sorted.findIndex((x) => x.title === "Batman 1"))).toBe("B");
  });

  it("shows only for title sorts of a whole listing of more than 30", () => {
    const p = fitParams(null, "movies");
    expect(showsJump(p, 31, false)).toBe(true);
    expect(showsJump(p, 30, false)).toBe(false);
    expect(showsJump(p, 9_000, true)).toBe(false);
    expect(showsJump({ ...p, sort: "year" }, 100, false)).toBe(false);
  });
});

describe("a library's sort and filters", () => {
  it("offers each kind its own sorts, in order", () => {
    expect(sortsFor("music")).toEqual(["title", "added", "user_rating", "last_played"]);
    expect(sortsFor("shows")).not.toContain("duration");
    expect(sortsFor("movies")).toContain("bitrate");
  });

  it("puts titles A to Z and everything else newest or highest first", () => {
    expect(defaultOrder("title")).toBe("asc");
    expect(defaultOrder("added")).toBe("desc");
  });

  it("drops what a kind doesn't have", () => {
    expect(fitParams({ sort: "duration", watched: false, genre: "Rock", resolution: "4k", dynamicRange: "hdr" }, "music")).toEqual({
      sort: "title",
      order: "asc",
      watched: null,
      genre: null,
      year: null,
      resolution: null,
      dynamicRange: null,
    });
    expect(fitParams({ sort: "year", order: "asc", watched: false }, "movies")).toMatchObject({ sort: "year", order: "asc", watched: false });
  });

  it("filters by resolution and dynamic range, a value kept that isn't the server's being none", () => {
    expect(fitParams({ resolution: "4k", dynamicRange: "dolby_vision" }, "shows")).toMatchObject({ resolution: "4k", dynamicRange: "dolby_vision" });
    const unknown = { resolution: "8k", dynamicRange: "hdr12" } as unknown as Partial<LibraryParams>;
    expect(fitParams(unknown, "movies")).toMatchObject({ resolution: null, dynamicRange: null });
    expect(hasFilter(params({ resolution: "sd" }))).toBe(true);
    expect(hasFilter(params({ dynamicRange: "sdr" }))).toBe(true);
    expect(cleared(params({ sort: "year", genre: "Drama", resolution: "720", dynamicRange: "hlg" }))).toEqual(params({ sort: "year" }));
    // Older servers send neither; tokens without words here aren't offered.
    expect(qualityChoices({ genres: [], years: [] })).toEqual({ resolutions: [], dynamicRanges: [] });
    expect(qualityChoices({ genres: [], years: [], resolutions: ["4k", "1080", "8k"], dynamic_ranges: ["hdr", "dolby_vision", "hdr10", "sdr"] })).toEqual({
      resolutions: ["4k", "1080"],
      dynamicRanges: ["hdr", "dolby_vision", "hdr10", "sdr"],
    });
  });

  it("badges the posters only when the listing is about the files' quality", () => {
    expect(showsBadges(params({}))).toBe(false);
    expect(showsBadges(params({ genre: "Drama", year: 1999, watched: false }))).toBe(false);
    expect(showsBadges(params({ sort: "bitrate", order: "desc" }))).toBe(true);
    expect(showsBadges(params({ resolution: "4k" }))).toBe(true);
    expect(showsBadges(params({ dynamicRange: "sdr" }))).toBe(true);
  });

  it("counts by kind", () => {
    expect(countLabel(1500, "movies")).toBe("1,500 movies");
    expect(countLabel(1, "shows")).toBe("1 show");
    expect(countLabel(12, "music")).toBe("12 artists");
    // Filtered: the noun agrees with the whole library.
    expect(countLabel(23, "movies", 1500)).toBe("23 of 1,500 movies");
    expect(countLabel(1, "shows", 1)).toBe("1 of 1 show");
  });

  it("keeps a library's resolution and dynamic range with its view", () => {
    const kept = new Map<string, string>();
    const kv = { get: (k: string) => kept.get(k) ?? null, set: (k: string, v: string) => void kept.set(k, v), remove: (k: string) => void kept.delete(k) };
    const quality = params({ resolution: "1080", dynamicRange: "hdr10plus" });
    new ParamsStore(kv, "http://nas:7539", 1).set(3, quality);
    expect(new ParamsStore(kv, "http://nas:7539", 1).get(3, "movies")).toEqual(quality);
    // A view kept before these filters reads as without them.
    kept.set("pley.library.http://nas:7539.1.4", JSON.stringify({ sort: "year", order: "asc", watched: null, genre: null, year: null }));
    expect(new ParamsStore(kv, "http://nas:7539", 1).get(4, "movies")).toEqual(params({ sort: "year", order: "asc" }));
  });

  it("keeps each profile's view of a library apart", () => {
    const kept = new Map<string, string>();
    const kv = { get: (k: string) => kept.get(k) ?? null, set: (k: string, v: string) => void kept.set(k, v), remove: (k: string) => void kept.delete(k) };
    const added = { ...fitParams(null, "movies"), sort: "added" as const, order: "desc" as const };
    new ParamsStore(kv, "http://nas:7539", 1).set(3, added);
    expect(new ParamsStore(kv, "http://nas:7539", 1).get(3, "movies").sort).toBe("added");
    expect(new ParamsStore(kv, "http://nas:7539", 2).get(3, "movies").sort).toBe("title");
    expect(new ParamsStore(kv, "http://other:7539", 1).get(3, "movies").sort).toBe("title");
    // Without a profile nothing is read or kept.
    new ParamsStore(kv, "http://nas:7539", null).set(4, added);
    expect(kept.size).toBe(1);
  });
});

describe("a library's tab", () => {
  const kv = () => {
    const kept = new Map<string, string>();
    return { kept, get: (k: string) => kept.get(k) ?? null, set: (k: string, v: string) => void kept.set(k, v), remove: (k: string) => void kept.delete(k) };
  };

  it("is kept for each server, profile and library", () => {
    const store = kv();
    new TabStore(store, "http://nas:7539", 1).set(3, "recommended");
    expect(new TabStore(store, "http://nas:7539", 1).get(3)).toBe("recommended");
    expect(new TabStore(store, "http://nas:7539", 1).get(4)).toBeNull();
    expect(new TabStore(store, "http://nas:7539", 2).get(3)).toBeNull();
    expect(new TabStore(store, "http://other:7539", 1).get(3)).toBeNull();
    // Without a profile nothing is read or kept.
    new TabStore(store, "http://nas:7539", null).set(3, "collections");
    expect(store.kept.size).toBe(1);
  });

  it("reads nothing from a value that isn't a tab", () => {
    const store = kv();
    store.set("pley.libraryTab.http://nas:7539.1.3", "upcoming");
    expect(new TabStore(store, "http://nas:7539", 1).get(3)).toBeNull();
  });

  it("opens on the tab shown before, else asked for, else last on, else Library", () => {
    const tabs: Parameters<typeof openingTab>[0] = ["recommended", "library", "collections"];
    expect(openingTab(tabs, undefined, undefined, "recommended")).toBe("recommended");
    expect(openingTab(tabs, undefined, "library", "recommended")).toBe("library");
    expect(openingTab(tabs, "collections", "library", "recommended")).toBe("collections");
    expect(openingTab(tabs, undefined, undefined, null)).toBe("library");
    // Music has no Collections tab.
    expect(openingTab(["recommended", "library"], undefined, undefined, "collections")).toBe("library");
  });
});

describe("Home", () => {
  const cw: Hub = { key: "continue_watching", title: "Continue Watching", library_id: null, items: [summary({ id: 5, kind: "episode", added_at: 10 })] };
  const movies: Hub = { key: "recently_added:1", title: "Recently Added in Movies", library_id: 1, items: [summary({ id: 6, added_at: 99 })] };
  const music: Hub = { key: "recently_added:3", title: "Recently Added in Music", library_id: 3, items: [summary({ id: 7, kind: "album", added_at: 200 })] };

  it("draws Continue Watching wide, music square, the rest as posters", () => {
    expect(rowKind(cw, [3])).toBe("landscape");
    expect(rowKind(music, [3])).toBe("square");
    expect(rowKind(movies, [3])).toBe("poster");
  });

  it("offers See all for a library's newest only", () => {
    expect(seeAllOf(movies)).toBe(1);
    expect(seeAllOf(cw)).toBeNull();
  });

  it("lists a row's See all in the row's order", () => {
    expect(seeAllSort(movies)).toBe("added");
    expect(seeAllSort({ ...movies, key: "recently_released:1" })).toBe("release");
    expect(seeAllSort({ ...movies, key: "top_rated:1" })).toBe("rating");
    expect(seeAllSort(cw)).toBeNull();
  });

  it("puts Continue Watching's first in the hero, else the newest video, when nothing is featured", () => {
    expect(heroOf([cw, movies])!.id).toBe(5);
    expect(heroOf([movies, music])!.id).toBe(6);
  });

  // The native app runs the same cases (HomeRulesTests).
  for (const c of featured.cases) {
    it(`features the newest movies and shows: ${c.name}`, () => {
      const picked = featuredOf(c.hubs as unknown as Hub[], "count" in c ? c.count : undefined);
      expect(picked.map((f) => ({ id: f.item.id, fromList: f.fromList }))).toEqual(c.featured);
    });
  }

  it("says why a featured title is there", () => {
    const libraries = [{ id: 1, name: "Movies", kind: "movies" }] as Library[];
    const film = summary({ id: 6, library_id: 1 });
    expect(featuredLabel({ item: film, fromList: false }, libraries)).toBe("New in Movies");
    expect(featuredLabel({ item: film, fromList: false }, null)).toBe("New");
    expect(featuredLabel({ item: film, fromList: true }, libraries)).toBe("On your list");
  });
});

describe("signing in", () => {
  it("shows a code as two halves", () => {
    expect(displayCode("bcdfghjk")).toBe("BCDF-GHJK");
    expect(codeHalves("BCDF-GHJK")).toEqual(["BCDF", "GHJK"]);
    expect(codeHalves("ABC")).toEqual(["ABC"]);
    expect(typedAddress("http://192.168.1.20:7539/link")).toBe("192.168.1.20:7539/link");
  });

  it("says why a profile switch was refused", () => {
    const refusal = (reason: string | null, error = "invalid_grant", description: string | null = null) =>
      switchRefusal(new TokenError(400, { error, description, reason } as OAuthError));
    expect(refusal("wrong_pin").message).toBe("Wrong PIN.");
    expect(refusal("pin_locked").locked).toBe(true);
    expect(refusal("browser").approve).toBe(true);
    expect(refusal(null, "invalid_request").letThrough).toBe(true);
    expect(refusal("throttled", "invalid_grant", "too many tries: try again in 5 minutes").message).toBe("Too many tries: try again in 5 minutes.");
    // A code without a description reads as words, not "Invalid_client."
    expect(refusal(null, "invalid_client").message).toBe("The server doesn't know this app.");
  });

  it("doesn't call a PIN wrong when the server answered nothing", () => {
    const r = switchRefusal(new NetworkError("Can't reach the server"));
    expect(r).toMatchObject({ offline: true, message: "Can't reach your server: nothing answers there." });
    expect(switchRefusal(new NetworkError("The server took too long to answer")).message).toBe("Can't reach your server: no answer in time.");
  });
});

describe("finding a server", () => {
  function http(answers: Record<string, number | "down" | "slow" | object>): Transport {
    return async (req: HttpRequest) => {
      const a = answers[req.url];
      if (a === undefined || a === "down") throw new NetworkError("Can't reach the server");
      if (a === "slow") throw new NetworkError("The server took too long to answer");
      if (typeof a === "number") return { status: a, header: () => null, body: null };
      return { status: 200, header: () => "application/json", body: a };
    };
  }

  it("names servers by host and port", () => {
    expect(hostPort("http://192.168.1.20:7539")).toBe("192.168.1.20:7539");
    expect(hostPort("https://pley.example.com")).toBe("pley.example.com");
    expect(subnetOf("192.168.1.42")).toBe("192.168.1.");
    expect(subnetOf("127.0.0.1")).toBeNull();
    expect(subnetOf(null)).toBeNull();
  });

  it("tries https first for a name on the internet, then pley's port", async () => {
    const t = http({ "http://pley.example.com:7539/api/server": { name: "Home", version: "1.0.0" } });
    await expect(connect(t, "pley.example.com")).resolves.toMatchObject({ url: "http://pley.example.com:7539", name: "Home" });
  });

  it("learns whether the server requires two-step sign-in", async () => {
    const required = http({ "http://192.168.1.20:7539/api/server": { name: "Den", version: "1.0.0", two_step_required: true } });
    await expect(check(required, "http://192.168.1.20:7539")).resolves.toMatchObject({ twoStepRequired: true });
    // Not required, or a server from before it.
    const older = http({ "http://192.168.1.20:7539/api/server": { name: "Den", version: "0.9.0" } });
    await expect(check(older, "http://192.168.1.20:7539")).resolves.toMatchObject({ twoStepRequired: false });
    const odd = http({ "http://192.168.1.20:7539/api/server": { name: "Den", version: "1.0.0", two_step_required: "yes" } });
    await expect(check(odd, "http://192.168.1.20:7539")).resolves.toMatchObject({ twoStepRequired: false });
  });

  it("says what answered when it isn't pley, and why nothing did", async () => {
    await expect(connect(http({ "http://router:7539/api/server": 404 }), "router")).rejects.toMatchObject({ problem: { kind: "not_pley", address: "router:7539" } });
    await expect(connect(http({}), "192.168.1.77")).rejects.toMatchObject({ problem: { kind: "unreachable", address: "192.168.1.77:7539", reason: "nothing answers there" } });
    await expect(check(http({ "http://nas:7539/api/server": "slow" }), "http://nas:7539")).rejects.toMatchObject({ problem: { reason: "no answer in time" } });
    await expect(connect(http({}), "not an address")).rejects.toMatchObject({ problem: { kind: "bad_address" } });
  });
});

describe("music", () => {
  it("shuffles with the chosen track first", () => {
    const list = [1, 2, 3, 4, 5, 6];
    const s = shuffled(list, 3, () => 0.5);
    expect(s[0]).toBe(4);
    expect(s.slice().sort()).toEqual(list);
  });
});

describe("watch history", () => {
  it("says when, in words", () => {
    const now = 1_800_000_000_000;
    expect(relativeTime(now / 1000 - 10, now)).toBe("now");
    expect(relativeTime(now / 1000 - 3 * 3600, now)).toBe("3 hours ago");
    expect(relativeTime(now / 1000 - 26 * 3600, now)).toBe("yesterday");
  });
});

describe("the people row", () => {
  const person = (id: number, role: Credit["role"], character?: string): Credit => ({ id, name: `P${id}`, role, character });
  const detail = (kind: ItemDetail["kind"], cast: Credit[], crew: Credit[]) => ({ kind, cast, crew }) as ItemDetail;
  const row = (d: ItemDetail) => {
    const r = castAndCrew(d);
    return r && { title: r.title, people: r.people.map((p) => `${p.credit.id}:${p.subtitle}`) };
  };

  it("follows the cast with each crew role's first four, nobody twice", () => {
    const crew = [person(1, "director"), person(9, "writer"), person(9, "writer"), ...[10, 11, 12, 13, 14].map((id) => person(id, "writer")), person(20, "creator")];
    expect(row(detail("movie", [person(1, "actor", "Ana"), person(2, "actor", "K")], crew))).toEqual({
      title: "Cast & Crew",
      people: ["1:Ana", "2:K", "20:Creator", "9:Writer", "10:Writer", "11:Writer", "12:Writer"],
    });
  });

  it("is titled by what it holds", () => {
    expect(row(detail("movie", [person(1, "actor")], []))!.title).toBe("Cast");
    expect(row(detail("episode", [person(1, "actor")], []))!.title).toBe("Guest Stars");
    expect(row(detail("episode", [person(1, "actor")], [person(2, "writer")]))!.title).toBe("Guest Stars & Crew");
    expect(row(detail("movie", [], [person(2, "director")]))!.title).toBe("Crew");
    // Producers aren't named.
    expect(row(detail("movie", [], [person(2, "producer")]))).toBeNull();
  });
});

describe("Save as… (smart playlists)", () => {
  it("writes the library's filters and order in Plex's syntax, as the web and Android do", () => {
    expect(smartFilterOf(params({ watched: false, genre: "Comedy", year: 1999 }))).toBe("genre=Comedy&year=1999&unwatched=1&sort=titleSort");
    expect(smartFilterOf(params({ sort: "year", order: "desc", watched: true }))).toBe("unwatched=0&sort=year%3Adesc");
    expect(smartFilterOf(params({ sort: "added", order: "desc", genre: "Science Fiction & Fantasy" }))).toBe("genre=Science+Fiction+%26+Fantasy&sort=addedAt%3Adesc");
    // Plex's resolution and Pley's dynamic range, after the year and before unwatched.
    expect(smartFilterOf(params({ watched: false, genre: "Drama", year: 1999, resolution: "4k", dynamicRange: "hdr" }))).toBe(
      "genre=Drama&year=1999&resolution=4k&dynamicRange=hdr&unwatched=1&sort=titleSort",
    );
    expect(smartFilterOf(params({ sort: "bitrate", order: "desc", dynamicRange: "dolby_vision" }))).toBe("dynamicRange=dolby_vision&sort=mediaBitrate%3Adesc");
  });

  it("says what a filter finds", () => {
    expect(describeSmartFilter("genre=Comedy&year=1999&unwatched=1&sort=titleSort")).toEqual(["Genre: Comedy", "From 1999", "Unwatched", "By title"]);
    expect(describeSmartFilter(smartFilterOf(params({ sort: "added", order: "desc", watched: true, genre: "Science Fiction & Fantasy" })))).toEqual([
      "Genre: Science Fiction & Fantasy",
      "Watched",
      "Recently added first",
    ]);
    expect(describeSmartFilter("type=1&sort=movie.year:desc&decade=1980")).toEqual(["Newest first", "The 1980s"]);
    // By their words, never the server's tokens.
    expect(describeSmartFilter("resolution=4k&dynamicRange=dolby_vision&sort=titleSort")).toEqual(["4K", "Dolby Vision", "By title"]);
    expect(describeSmartFilter("resolution=1080&dynamicRange=hdr10plus")).toEqual(["1080p", "HDR+"]);
    expect(describeSmartFilter("resolution=480")).toEqual(["resolution: 480"]);
  });
});

describe("season tabs", () => {
  // Shared with the native Samsung app's tests.
  for (const c of seasonPosterCases.cases) {
    it(`show posters: ${c.name}`, () => {
      expect(seasonPosters(c.posters.map((poster_url) => ({ poster_url })))).toBe(c.posterTabs);
    });
  }
});
