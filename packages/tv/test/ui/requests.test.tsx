import { signal } from "@preact/signals";
import { render, type ComponentChildren } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, type RequestQuery } from "../../src/api/client";
import type { DiscoverDetail, DiscoverPage, DiscoverSeason, DiscoverTitle, ItemRequest, MediaRequest, NewRequest, RequestList, RequestSummary, ServerEvent, User } from "../../src/api/types";
import type { KeyValue } from "../../src/auth/tokens";
import type { Platform } from "../../src/platform";
import { AppState } from "../../src/state";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import { F, G } from "../../src/ui/kit/focusable";
import type { Key } from "../../src/ui/keys";
import { AppContext } from "../../src/ui/screens/common";
import { DiscoverListScreen, DiscoverScreen } from "../../src/ui/screens/discover";
import { ItemRequestLine, useItemRequest } from "../../src/ui/screens/request-kit";
import { RequestsScreen } from "../../src/ui/screens/requests-page";
import { SearchScreen } from "../../src/ui/screens/search";
import { TitleScreen } from "../../src/ui/screens/title";
import { RequestNotices } from "../../src/ui/shell/notice";
import type { Entry, PageKey } from "../../src/ui/shell/navigator";

/**
 * Requests on the TV, as the remote uses them: Discover and its search, a
 * title's page and its season picker, the lists of requests and what may
 * be done with one, On TMDB in search, a library title's request, and
 * ready-to-watch news. Focus is never lost on the way.
 */

/** Friday 30 October 2026, noon on the TV's clock. */
const NOW = new Date(2026, 9, 30, 12, 0, 0).getTime();
const AT = Math.floor(NOW / 1000);
const DAY = 86_400;
/** What keeps a line's last two words together. */
const NBSP = String.fromCharCode(0xa0);

function memoryStore(): KeyValue {
  const kept = new Map<string, string>();
  return {
    get: (k) => kept.get(k) ?? null,
    set: (k, v) => void kept.set(k, v),
    remove: (k) => void kept.delete(k),
  };
}

function summary(over: Partial<RequestSummary> = {}): RequestSummary {
  return {
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
  };
}

function request(over: Partial<MediaRequest> = {}): MediaRequest {
  return {
    id: 1,
    kind: "movie",
    tmdb_id: 10,
    title: "Arrival",
    year: 2016,
    poster_url: null,
    backdrop_url: null,
    seasons: null,
    available_seasons: [],
    anime: false,
    status: "pending",
    stage: "pending",
    stage_at: null,
    reason: null,
    note: null,
    mine: true,
    watching: false,
    watchers: 0,
    unseen: false,
    actions: [],
    requested_by: null,
    decided_by: null,
    decided_at: null,
    created_at: AT - 3 * DAY,
    updated_at: AT,
    available_at: null,
    retry_at: null,
    sent_to: null,
    item_id: null,
    download: null,
    ...over,
  };
}

function title(over: Partial<DiscoverTitle> = {}): DiscoverTitle {
  return { kind: "movie", tmdb_id: 10, title: "Arrival", year: 2016, overview: "", poster_url: null, backdrop_url: null, rating: 7.9, status: "none", item_id: null, request_id: null, ...over };
}

function season(number: number, state: DiscoverSeason["state"], over: Partial<DiscoverSeason> = {}): DiscoverSeason {
  return { number, name: number ? `Season ${number}` : "Specials", episodes: 10, aired: 10, air_date: null, poster_url: null, in_library: 0, requested: false, state, declined: false, request_id: null, ...over };
}

function detail(over: Partial<DiscoverDetail> = {}): DiscoverDetail {
  return {
    ...title(),
    original_title: null,
    tagline: null,
    genres: [],
    release_date: null,
    runtime_minutes: 116,
    certification: null,
    network: null,
    show_status: null,
    anime: false,
    cast: [],
    seasons: null,
    imdb_id: null,
    declined: null,
    restriction: null,
    watching: false,
    requests: [],
    movies_left: null,
    seasons_left: null,
    movies_next_at: null,
    seasons_next_at: null,
    ...over,
  };
}

function show(seasons: DiscoverSeason[], over: Partial<DiscoverDetail> = {}): DiscoverDetail {
  return detail({ kind: "show", tmdb_id: 20, title: "Severance", runtime_minutes: null, seasons, ...over });
}

function page(results: DiscoverTitle[], n = 1, total = 1): DiscoverPage {
  return { page: n, total_pages: total, results };
}

function list(requests: MediaRequest[]): RequestList {
  const counts = { pending: 0, approved: 0, declined: 0, failed: 0, available: 0 };
  for (const r of requests) counts[r.status]++;
  return { total: requests.length, counts, requests, people: [] };
}

let detach: (() => void) | null = null;
let root: HTMLElement | null = null;

/** One screen with the app's state and a fake server; the shell's parts (navigation, toasts, the rail) as lists. */
function mount(screen: (entry: Entry, app: AppState) => ComponentChildren, api: Record<string, unknown>, s: RequestSummary | null, key: PageKey = { name: "discover" }) {
  // A test that mounts again ends the screen before.
  detach?.();
  if (root) act(() => render(null, root!));
  document.body.innerHTML = "";
  root = document.createElement("div");
  // The shell's page root, which pages look for when focus went with what they removed.
  root.className = "screen";
  document.body.appendChild(root);
  const platform = { name: "webos", clientId: "test", storage: memoryStore() } as unknown as Platform;
  const app = new AppState(platform, () => Promise.reject(new Error("no network in tests")));
  (app.api as { value: unknown }).value = api;
  app.server.value = "http://pley.test";
  app.me.value = { id: 1 } as User;
  app.requests.value = s;
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  detach = input.attach(document);
  const toasts: string[] = [];
  const opened: PageKey[] = [];
  let railed = 0;
  const images = { url: (u: string) => u, ready: () => null, isMissing: () => true, is: () => false, load: () => () => undefined };
  const ui = {
    focus,
    input,
    nav: { open: (k: PageKey) => void opened.push(k), pop: () => true, replace: () => undefined, restart: () => undefined },
    api,
    images,
    toast: (t: string) => void toasts.push(t),
    claim: (el: HTMLElement, _fid: string | null, initial: boolean) => {
      const current = focus.current;
      if (initial && focus.inScope(el) && !(current && current.isConnected && focus.inScope(current))) focus.focus(el);
    },
    toRail: () => {
      railed++;
      return true;
    },
    scale: 1,
    tier: "high",
    reducedMotion: false,
  } as unknown as Ui;
  const entry: Entry = { key, id: 1, memory: null, state: {} };
  act(() =>
    render(
      <AppContext.Provider value={app}>
        <UiContext.Provider value={ui}>{screen(entry, app)}</UiContext.Provider>
      </AppContext.Provider>,
      root!,
    ),
  );
  const el = (fid: string) => document.querySelector<HTMLElement>(`[data-fid="${fid}"]`);
  const press = (k: Key) => {
    act(() => input.dispatch(k, "down", false));
    act(() => input.dispatch(k, "up", false));
    act(() => void vi.advanceTimersByTime(150));
  };
  const select = (fid: string) => {
    act(() => void focus.focus(el(fid)));
    press("ok");
  };
  /** The answers on their way. */
  const settle = () =>
    act(async () => {
      for (let i = 0; i < 10; i++) await Promise.resolve();
    });
  const advance = (ms: number) => act(() => void vi.advanceTimersByTime(ms));
  const fid = () => focus.current?.getAttribute("data-fid") ?? null;
  const text = (selector: string) => Array.from(document.querySelectorAll(selector)).map((e) => e.textContent ?? "");
  /** Words in a field, as the TV's keyboard types them. */
  const type = (selector: string, words: string) => {
    const box = document.querySelector<HTMLInputElement>(selector)!;
    box.value = words;
    act(() => void box.dispatchEvent(new Event("input", { bubbles: true })));
  };
  return { app, focus, input, toasts, opened, railed: () => railed, el, press, select, settle, advance, fid, text, type };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
});
afterEach(() => {
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root = null;
  vi.useRealTimers();
});

describe("a title's page", () => {
  it("asks for a movie from its one main action, then says Requested beside its stage, focus where it was", async () => {
    let d = detail();
    const asked: NewRequest[] = [];
    const t = mount(
      (entry) => <TitleScreen entry={entry} kind="movie" tmdbId={10} />,
      {
        discoverTitle: () => Promise.resolve(d),
        askFor: (body: NewRequest) => {
          asked.push(body);
          const r = request({ id: 5, actions: ["withdraw"] });
          d = detail({ status: "requested", request_id: 5, requests: [r] });
          return Promise.resolve({ request: r, created: true });
        },
      },
      summary(),
    );
    await t.settle();
    expect(t.fid()).toBe("main");
    expect(t.el("main")!.textContent).toBe("Request");
    t.press("ok");
    await t.settle();
    expect(asked).toEqual([{ kind: "movie", tmdb_id: 10 }]);
    expect(t.toasts).toEqual(["Asked for “Arrival”: an administrator decides."]);
    // The page asks again once the change is in, and the same button says where it is.
    t.advance(300);
    await t.settle();
    expect(t.el("main")!.textContent).toBe("Requested");
    expect(t.fid()).toBe("main");
    expect(t.text(".title-page .stage-pill")).toEqual(["Waiting for approval"]);
  });

  it("asks for more of a show in the picker: the missing aired seasons ticked, the button counting them, focus back after", async () => {
    const d = show([season(1, "library"), season(2, "missing"), season(3, "missing"), season(4, "unaired", { aired: 0 }), season(0, "missing")], { status: "partial", item_id: 7 });
    const asked: NewRequest[] = [];
    const t = mount(
      (entry) => <TitleScreen entry={entry} kind="show" tmdbId={20} />,
      {
        discoverTitle: () => Promise.resolve(d),
        askFor: (body: NewRequest) => {
          asked.push(body);
          return Promise.resolve({ request: request({ id: 6, kind: "show", tmdb_id: 20, title: "Severance", seasons: body.seasons ?? null }), created: true });
        },
      },
      summary(),
    );
    await t.settle();
    // Partly in the library: Open first, then Request more seasons.
    expect(t.fid()).toBe("main");
    expect(t.el("main")!.textContent).toBe("Open");
    t.press("right");
    expect(t.fid()).toBe("more");
    t.press("ok");
    const ticked = () => Array.from(document.querySelectorAll(".side-panel .panel-row")).filter((r) => r.querySelector(".tick.on")).map((r) => r.getAttribute("data-fid"));
    expect(t.text(".side-panel .panel-title")).toEqual(["Which seasons?"]);
    expect(ticked()).toEqual(["pick/2", "pick/3"]);
    // In the library: it says so and can't be ticked; not aired yet and specials are offered unticked.
    expect(t.el("pick/1")!.hasAttribute("data-fdisabled")).toBe(true);
    expect(t.el("pick/4")!.hasAttribute("data-fdisabled")).toBe(false);
    expect(t.fid()).toBe("pick/2");
    expect(t.el("pick/ask")!.textContent).toBe("Request 2 seasons");
    t.press("ok");
    expect(ticked()).toEqual(["pick/3"]);
    expect(t.el("pick/ask")!.textContent).toBe("Request 1 season");
    t.select("pick/ask");
    await t.settle();
    expect(asked).toEqual([{ kind: "show", tmdb_id: 20, seasons: [3] }]);
    expect(t.toasts).toEqual(["Asked for “Severance”: an administrator decides."]);
    expect(document.querySelector(".side-panel")).toBeNull();
    expect(t.fid()).toBe("more");
  });

  it("turns the picker's button off when more seasons are ticked than may be asked for, and says how many may", async () => {
    const d = show([season(1, "missing"), season(2, "missing"), season(3, "missing")], { seasons_left: 2 });
    const t = mount((entry) => <TitleScreen entry={entry} kind="show" tmdbId={20} />, { discoverTitle: () => Promise.resolve(d) }, summary({ seasons_left: 2, seasons_limit: 10 }));
    await t.settle();
    t.press("ok");
    // As many ticked as may be asked for: nothing to say yet.
    expect(t.el("pick/ask")!.textContent).toBe("Request 2 seasons");
    expect(document.querySelector(".picker-note")).toBeNull();
    t.select("pick/3");
    expect(t.el("pick/ask")!.hasAttribute("data-fdisabled")).toBe(true);
    expect(t.text(".picker-over")).toEqual([`You can ask for 2 more${NBSP}seasons.`]);
    expect(t.text(".picker-note > div")).toEqual([`You can ask for 2 more${NBSP}seasons.`, `Limits: 10 seasons per 7${NBSP}days.`]);
  });

  it("says why a declined title can't be asked for, with no button, over the other buttons, and gives focus to the requests", async () => {
    const d = detail({ status: "declined", declined: { reason: "Not this one.", at: AT - DAY, again_at: AT + 12 * DAY } });
    const t = mount((entry) => <TitleScreen entry={entry} kind="movie" tmdbId={10} />, { discoverTitle: () => Promise.resolve(d) }, summary({ own: 1 }));
    await t.settle();
    expect(t.el("main")).toBeNull();
    // Its last two words kept together, so the line never ends with one alone.
    expect(t.text(".title-page .title-note")).toEqual([`Declined: “Not this one”. You can ask again in 12${NBSP}days.`]);
    expect(document.querySelector(".title-page .title-notes")!.nextElementSibling!.classList.contains("buttons")).toBe(true);
    expect(t.fid()).toBe("requests");
  });

  it("disables Request when nothing is left to ask for, and says when there will be", async () => {
    const d = detail({ movies_left: 0, movies_next_at: AT + 3 * DAY });
    const t = mount((entry) => <TitleScreen entry={entry} kind="movie" tmdbId={10} />, { discoverTitle: () => Promise.resolve(d) }, summary({ movies_left: 0, movies_limit: 3, movies_next_at: AT + 3 * DAY }));
    await t.settle();
    expect(t.el("main")!.hasAttribute("data-fdisabled")).toBe(true);
    expect(t.text(".title-page .title-note")).toEqual([`You've asked for all the movies you can for now: you can ask for more in 3${NBSP}days.`, `Limits: 3 movies per 7${NBSP}days.`]);
    // Under the disabled button, as what it says of it.
    expect(document.querySelector(".title-page .title-notes")!.previousElementSibling!.classList.contains("buttons")).toBe(true);
    // The remote isn't dead: the requests take focus.
    expect(t.fid()).toBe("requests");
  });

  it("takes TMDB not answering for an answer, with Retry, the server not away; a title TMDB doesn't know says so", async () => {
    let fail = true;
    const down = new ApiError(503, "tmdb_down", "TMDB isn't answering", { code: "tmdb_down", message: "TMDB isn't answering", setup: true });
    const t = mount((entry) => <TitleScreen entry={entry} kind="movie" tmdbId={10} />, { discoverTitle: () => (fail ? Promise.reject(down) : Promise.resolve(detail())) }, summary());
    await t.settle();
    expect(t.text(".message-page .t-page")).toEqual(["TMDB isn't answering"]);
    expect(t.app.offline.value).toBe(false);
    fail = false;
    t.press("ok");
    await t.settle();
    expect(t.el("main")!.textContent).toBe("Request");

    const gone = mount((entry) => <TitleScreen entry={entry} kind="movie" tmdbId={11} />, { discoverTitle: () => Promise.reject(new ApiError(404, "not_found", "not found", { code: "not_found" })) }, summary());
    await gone.settle();
    expect(gone.text(".message-page .t-page")).toEqual(["Not on TMDB"]);
  });

  it("follows its download every 10 s while one moves, and stops asking once it is in", async () => {
    let asked = 0;
    let stage: MediaRequest["stage"] = "downloading";
    let progress = 0.25;
    const now = () => {
      if (stage === "available") return detail({ status: "library", item_id: 3 });
      const r = request({ id: 5, status: "approved", stage, download: { progress, state: "downloading", eta: null, episodes: 1 } });
      return detail({ status: "requested", request_id: 5, requests: [r] });
    };
    const t = mount(
      (entry) => <TitleScreen entry={entry} kind="movie" tmdbId={10} />,
      {
        discoverTitle: () => {
          asked++;
          return Promise.resolve(now());
        },
      },
      summary(),
    );
    await t.settle();
    expect(t.text(".title-page .stage-pill")).toEqual(["Downloading 25%"]);
    expect(document.querySelector(".title-page .download-bar")!.getAttribute("aria-valuenow")).toBe("25");
    progress = 0.5;
    t.advance(10_000);
    await t.settle();
    expect(t.text(".title-page .stage-pill")).toEqual(["Downloading 50%"]);
    expect(t.fid()).toBe("main");
    stage = "available";
    t.advance(10_000);
    await t.settle();
    expect(t.el("main")!.textContent).toBe("Open");
    expect(t.fid()).toBe("main");
    const calls = asked;
    t.advance(60_000);
    await t.settle();
    expect(asked).toBe(calls);
  });

  it("puts a season's download time on its own line under its name, without the dot", async () => {
    const r = request({ id: 5, kind: "show", tmdb_id: 20, title: "Severance", seasons: [2], status: "approved", stage: "downloading", download: { progress: 0.7, state: "downloading", eta: AT + 2 * 3600, episodes: 10 } });
    const d = show([season(1, "library"), season(2, "requested", { request_id: 5 })], { status: "partial", item_id: 7, requests: [r] });
    const t = mount((entry) => <TitleScreen entry={entry} kind="show" tmdbId={20} />, { discoverTitle: () => Promise.resolve(d) }, summary());
    await t.settle();
    expect(document.querySelector('[data-fid="season/2"] .season-caption .sub')!.innerHTML).toBe("Downloading 70%<br>in 2 hours");
    expect(t.el("season/2")!.getAttribute("aria-label")).toBe("Season 2, Requested, Downloading 70% · in 2 hours");
  });
});

describe("Discover", () => {
  const TITLES = [title({ tmdb_id: 1, title: "Arrival" }), title({ tmdb_id: 2, title: "Dune", status: "library", item_id: 3 }), title({ tmdb_id: 3, title: "Heat", status: "requested", request_id: 9 })];

  it("leads with the first trending title and its main action; the rows' posters carry their badges", async () => {
    const lists: string[] = [];
    const t = mount(
      (entry) => <DiscoverScreen entry={entry} />,
      {
        discover: (l: string, n: number) => {
          lists.push(l);
          return Promise.resolve(page(l === "trending" ? TITLES : [], n));
        },
        discoverTitle: () => Promise.resolve(detail({ tmdb_id: 1 })),
      },
      summary(),
    );
    await t.settle();
    expect(t.text(".discover-hero .t-hero")).toEqual(["Arrival"]);
    expect(t.fid()).toBe("hero/main");
    expect(t.el("hero/main")!.textContent).toBe("Request");
    expect(t.text(".title-badge")).toEqual(["In library", "Requested"]);
    // The first rows load at once; the others as focus comes near them.
    expect(lists).toEqual(["trending", "movies"]);
  });

  it("searches TMDB as typing pauses, in place of the rows; Back empties the words and the rows come back", async () => {
    const searched: unknown[][] = [];
    const t = mount(
      (entry) => <DiscoverScreen entry={entry} />,
      {
        discover: (l: string, n: number) => Promise.resolve(page(l === "trending" ? TITLES : [], n)),
        discoverTitle: () => Promise.resolve(detail({ tmdb_id: 1 })),
        discoverSearch: (q: string, n: number, missing: boolean) => {
          searched.push([q, n, missing]);
          return Promise.resolve(page(q === "arr" ? [TITLES[0]!] : [], n));
        },
      },
      summary(),
    );
    await t.settle();
    act(() => void t.focus.focus(t.el("search")));
    t.type('[data-fid="search"] input', "arr");
    t.advance(399);
    expect(searched).toEqual([]);
    t.advance(1);
    await t.settle();
    expect(searched).toEqual([["arr", 1, false]]);
    expect(t.text(".discover-results .tile-caption .title")).toEqual(["Arrival"]);
    t.type('[data-fid="search"] input', "zzz");
    t.advance(400);
    await t.settle();
    expect(t.text(".discover-note .t-title")).toEqual(["Nothing on TMDB for “zzz”"]);
    t.press("back");
    expect(t.el("hero/main")).not.toBeNull();
    expect(t.fid()).toBe("search");
  });

  it("reads at most 5 pages for a row, then shows what they had; a row they had nothing for goes", async () => {
    const asked: string[] = [];
    const t = mount(
      (entry) => <DiscoverScreen entry={entry} />,
      {
        discover: (l: string, n: number) => {
          asked.push(`${l} ${n}`);
          // An age limit leaves out most of every list: TMDB has pages and pages, nearly all of them empty here.
          return Promise.resolve(page(l === "trending" && n === 2 ? TITLES : [], n, 500));
        },
        discoverTitle: () => Promise.resolve(detail({ tmdb_id: 1 })),
      },
      summary(),
    );
    for (let i = 0; i < 4; i++) await t.settle();
    const of = (list: string) => asked.filter((a) => a.split(" ")[0] === list);
    expect(of("trending")).toEqual(["trending 1", "trending 2", "trending 3", "trending 4", "trending 5"]);
    expect(of("movies")).toEqual(["movies 1", "movies 2", "movies 3", "movies 4", "movies 5"]);
    expect(t.text(".discover-hero .t-hero")).toEqual(["Arrival"]);
    expect(t.text(".row-header")).toContain("Trending this week");
    expect(t.text(".row-header")).not.toContain("Popular movies");
    t.advance(60_000);
    await t.settle();
    expect(asked.length).toBe(10);
  });

  it("ends a grid in Show more once 5 pages brought too few: it reads 5 more, focus staying on the way", async () => {
    const asked: number[] = [];
    const t = mount(
      (entry) => <DiscoverListScreen entry={entry} list="movies" />,
      {
        discover: (_l: string, n: number) => {
          asked.push(n);
          return Promise.resolve(page(n === 1 ? TITLES.slice(0, 2) : n === 6 ? [TITLES[2]!] : [], n, 500));
        },
      },
      summary(),
      { name: "discoverList", list: "movies" },
    );
    for (let i = 0; i < 4; i++) await t.settle();
    expect(asked).toEqual([1, 2, 3, 4, 5]);
    expect(t.text(".grid-cells .tile-caption .title")).toEqual(["Arrival", "Dune"]);
    expect(t.el("movies/more")!.getAttribute("aria-label")).toBe("Show more");
    t.select("movies/more");
    // The well in its place holds focus until the next title comes, which takes it.
    expect(t.fid()).toBe("movies/more");
    for (let i = 0; i < 4; i++) await t.settle();
    expect(asked).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    expect(t.text(".grid-cells .tile-caption .title")).toEqual(["Arrival", "Dune", "Heat"]);
    expect(t.fid()).toBe("movies/movie:3");
    expect(t.el("movies/more")!.getAttribute("aria-label")).toBe("Show more");
  });

  it("ends a grid in Retry when a later page fails, which reads on; back on the page, what failed is asked again", async () => {
    let fail = true;
    const asked: number[] = [];
    const down = new ApiError(503, "tmdb_down", "TMDB isn't answering", { code: "tmdb_down", message: "TMDB isn't answering", setup: true });
    const api = {
      discover: (_l: string, n: number) => {
        asked.push(n);
        if (n === 2 && fail) return Promise.reject(down);
        return Promise.resolve(page(n === 1 ? TITLES.slice(0, 2) : [TITLES[2]!], n, 2));
      },
    };
    let kept: Record<string, unknown> = {};
    const t = mount(
      (entry) => {
        kept = entry.state;
        return <DiscoverListScreen entry={entry} list="movies" />;
      },
      api,
      summary(),
      { name: "discoverList", list: "movies" },
    );
    for (let i = 0; i < 3; i++) await t.settle();
    expect(asked).toEqual([1, 2]);
    expect(t.text(".grid-cells .tile-caption .title")).toEqual(["Arrival", "Dune"]);
    expect(t.el("movies/more")!.getAttribute("aria-label")).toBe("Retry");
    // Left as it is, then shown again (Back to it): asked again by itself.
    const again = mount(
      (entry) => {
        entry.state = kept;
        return <DiscoverListScreen entry={entry} list="movies" />;
      },
      api,
      summary(),
      { name: "discoverList", list: "movies" },
    );
    for (let i = 0; i < 3; i++) await again.settle();
    expect(asked).toEqual([1, 2, 2]);
    expect(again.el("movies/more")!.getAttribute("aria-label")).toBe("Retry");
    // Retry reads on, focus going to what came in its place.
    fail = false;
    again.select("movies/more");
    for (let i = 0; i < 3; i++) await again.settle();
    expect(asked).toEqual([1, 2, 2, 2]);
    expect(again.text(".grid-cells .tile-caption .title")).toEqual(["Arrival", "Dune", "Heat"]);
    expect(again.fid()).toBe("movies/movie:3");
    expect(again.el("movies/more")).toBeNull();
  });

  it("says why a search has nothing yet beside its Show more, which keeps focus while it reads on", async () => {
    const asked: number[] = [];
    const t = mount(
      (entry) => <DiscoverScreen entry={entry} />,
      {
        discover: (_l: string, n: number) => Promise.resolve(page(TITLES, n)),
        discoverTitle: () => Promise.resolve(detail({ tmdb_id: 1 })),
        discoverSearch: (_q: string, n: number) => {
          asked.push(n);
          return Promise.resolve(page([], n, 500));
        },
      },
      summary(),
    );
    await t.settle();
    act(() => void t.focus.focus(t.el("search")));
    t.type('[data-fid="search"] input', "saw");
    t.advance(400);
    for (let i = 0; i < 4; i++) await t.settle();
    expect(asked).toEqual([1, 2, 3, 4, 5]);
    expect(t.text(".discover-aside .t-title")).toEqual(["Nothing yet for “saw”"]);
    t.select("result/more");
    expect(document.querySelector(".discover-aside")).toBeNull();
    expect(t.fid()).toBe("result/more");
    for (let i = 0; i < 4; i++) await t.settle();
    expect(asked.length).toBe(10);
    expect(t.fid()).toBe("result/more");
    expect(t.focus.current!.getAttribute("aria-label")).toBe("Show more");
  });

  it("says when requests are off, and leaves focus with the rail", () => {
    const t = mount((entry) => <DiscoverScreen entry={entry} />, {}, summary({ enabled: false }));
    expect(t.text(".message-page .t-page")).toEqual(["Requests are off"]);
    t.advance(20);
    expect(t.railed()).toBe(1);
  });
});

describe("the requests", () => {
  const ANN = { id: 2, name: "Ann", avatar_url: null, avatar_color: null };

  it("open on Needs attention for administrators, oldest first; Approve takes a card off it and focus goes to the next", async () => {
    const actions: MediaRequest["actions"] = ["approve", "decline", "delete"];
    let all = [request({ id: 1, mine: false, requested_by: ANN, actions }), request({ id: 2, tmdb_id: 11, title: "Dune", mine: false, requested_by: ANN, actions })];
    const queries: RequestQuery[] = [];
    const approved: number[] = [];
    const t = mount(
      (entry) => <RequestsScreen entry={entry} />,
      {
        requests: (q: RequestQuery) => {
          queries.push(q);
          return Promise.resolve(list(all.filter((r) => !q.attention || r.status === "pending")));
        },
        approveRequest: (id: number) => {
          approved.push(id);
          all = all.map((r) => (r.id === id ? { ...r, status: "approved", stage: "approved", actions: ["delete"] } : r));
          return Promise.resolve(all.filter((r) => r.id === id)[0]!);
        },
        markRequestsSeen: () => Promise.resolve(),
      },
      summary({ manage: true, pending: 2 }),
      { name: "requests" },
    );
    await t.settle();
    expect(t.text(".requests-tabs .tab > .one-line")).toEqual(["Needs attention", "All", "Pending", "Approved", "Available", "Declined", "Failed"]);
    expect(queries[0]).toEqual({ attention: true, order: "oldest", offset: 0, limit: 30 });
    expect(t.fid()).toBe("req/1");
    t.press("ok");
    expect(t.text(".side-panel .panel-row")).toEqual(["Approve", "Decline", "Decline with a reason…", "Delete", "Details"]);
    expect(t.fid()).toBe("action/approve");
    t.press("ok");
    await t.settle();
    expect(approved).toEqual([1]);
    expect(t.toasts).toEqual(["Approved “Arrival”."]);
    expect(t.el("req/1")).toBeNull();
    expect(t.fid()).toBe("req/2");
  });

  it("declines with a reason typed on the TV's keyboard: its Done goes to Decline, which sends it", async () => {
    let all = [request({ id: 2, tmdb_id: 11, title: "Dune", mine: false, requested_by: ANN, actions: ["approve", "decline", "delete"] })];
    const declined: [number, string | null][] = [];
    const t = mount(
      (entry) => <RequestsScreen entry={entry} />,
      {
        requests: (q: RequestQuery) => Promise.resolve(list(all.filter((r) => !q.attention || r.status === "pending"))),
        declineRequest: (id: number, reason: string | null) => {
          declined.push([id, reason]);
          all = all.map((r) => (r.id === id ? { ...r, status: "declined", stage: "declined", reason, actions: ["delete"] } : r));
          return Promise.resolve(all[0]!);
        },
        markRequestsSeen: () => Promise.resolve(),
      },
      summary({ manage: true, pending: 1 }),
      { name: "requests" },
    );
    await t.settle();
    t.press("ok");
    t.select("action/decline_reason");
    expect(t.text(".side-panel .panel-title")).toEqual(["Decline “Dune”?"]);
    // The keyboard is up on the reason as the step shows.
    expect(t.fid()).toBe("reason");
    expect(t.input.typing).not.toBeNull();
    t.type('[data-fid="reason"] input', "Not this one");
    t.press("ok");
    expect(t.input.typing).toBeNull();
    expect(t.fid()).toBe("decline");
    expect(declined).toEqual([]);
    t.press("ok");
    await t.settle();
    expect(declined).toEqual([[2, "Not this one"]]);
    expect(t.toasts).toEqual(["Declined “Dune”."]);
    // Nothing else needs attention: the tab has focus, and the list says so.
    expect(t.fid()).toBe("tab/attention");
    expect(t.text(".requests-note .t-section")).toEqual(["No requests need attention."]);
  });

  it("withdraws one's own request once asked in the panel; Cancel, first, and Back go back to the actions", async () => {
    let all = [request({ id: 1, actions: ["withdraw"] }), request({ id: 2, tmdb_id: 11, title: "Dune", actions: ["withdraw"] })];
    const deleted: number[] = [];
    const t = mount(
      (entry) => <RequestsScreen entry={entry} />,
      {
        requests: () => Promise.resolve(list(all)),
        deleteRequest: (id: number) => {
          deleted.push(id);
          all = all.filter((r) => r.id !== id);
          return Promise.resolve(null);
        },
        markRequestsSeen: () => Promise.resolve(),
      },
      summary({ own: 2 }),
      { name: "requests" },
    );
    await t.settle();
    expect(t.text(".requests-tabs .tab > .one-line")[0]).toBe("All");
    expect(t.fid()).toBe("req/1");
    t.press("ok");
    t.select("action/withdraw");
    expect(t.text(".side-panel .panel-title")).toEqual(["Withdraw your request for “Arrival”?"]);
    // OK pressed twice withdraws nothing.
    expect(t.fid()).toBe("cancel");
    t.press("ok");
    expect(t.fid()).toBe("action/withdraw");
    expect(deleted).toEqual([]);
    t.press("ok");
    t.press("back");
    expect(t.fid()).toBe("action/withdraw");
    t.press("ok");
    t.press("down");
    expect(t.fid()).toBe("confirm");
    t.press("ok");
    await t.settle();
    expect(deleted).toEqual([1]);
    expect(t.toasts).toEqual(["Withdrew your request for “Arrival”."]);
    expect(t.fid()).toBe("req/2");
  });

  it("fades out at the screen's foot while cards are below it, and not once the last is in view", async () => {
    const all = [1, 2, 3, 4, 5, 6].map((id) => request({ id, tmdb_id: 10 + id, title: `Title ${id}` }));
    const t = mount(
      (entry) => <RequestsScreen entry={entry} />,
      { requests: () => Promise.resolve(list(all)), markRequestsSeen: () => Promise.resolve() },
      summary({ own: 6 }),
      { name: "requests" },
    );
    await t.settle();
    const fading = () => document.querySelector(".requests-fade")!.classList.contains("shown");
    expect(t.fid()).toBe("req/1");
    expect(fading()).toBe(true);
    for (let i = 0; i < 5; i++) t.press("down");
    expect(t.fid()).toBe("req/6");
    expect(fading()).toBe(false);
  });
});

describe("search", () => {
  it("shows On TMDB after the libraries' results for whoever may ask, with See all on Discover", async () => {
    const asked: unknown[][] = [];
    const t = mount(
      (entry) => <SearchScreen entry={entry} />,
      {
        search: () => Promise.resolve({ items: [], people: [] }),
        discoverSearch: (q: string, n: number, missing: boolean) => {
          asked.push([q, n, missing]);
          return Promise.resolve(page([title({ tmdb_id: 1, title: "Arrival" })], n));
        },
      },
      summary(),
      { name: "search" },
    );
    t.type(".search-field input", "arr");
    t.advance(250);
    await t.settle();
    expect(asked).toEqual([["arr", 1, true]]);
    expect(t.text(".search-none")).toEqual(["Nothing in your libraries matches"]);
    expect(t.text(".search-rows .row-header")).toEqual(["On TMDB"]);
    t.select("tmdb/movie:1");
    t.select("tmdb/all");
    expect(t.opened).toEqual([
      { name: "title", kind: "movie", tmdbId: 1 },
      { name: "discover", query: "arr" },
    ]);
  });

  it("leaves TMDB out for a profile that may not ask", async () => {
    const asked: unknown[][] = [];
    const t = mount(
      (entry) => <SearchScreen entry={entry} />,
      {
        search: () => Promise.resolve({ items: [], people: [] }),
        discoverSearch: (q: string) => {
          asked.push([q]);
          return Promise.resolve(page([]));
        },
      },
      summary({ access: "off", own: 2 }),
      { name: "search" },
    );
    t.type(".search-field input", "arr");
    t.advance(250);
    await t.settle();
    expect(asked).toEqual([]);
    expect(t.text(".search-notice .t-title")).toEqual(["No results for “arr”"]);
  });
});

function Probe(p: { id: number; kind: "movie" | "show" | "season" }) {
  const r = useItemRequest(p.id, p.kind);
  return <div class="probe">{r && r.request ? <ItemRequestLine request={r.request} /> : null}</div>;
}

describe("a library title's request", () => {
  const ANSWER: ItemRequest = { kind: "show", tmdb_id: 20, status: "partial", request: request({ id: 4, kind: "show", tmdb_id: 20, status: "approved", stage: "approved" }), askable: [3] };

  it("is asked for once the page has drawn, never before, and shows its stage", async () => {
    const calls: number[] = [];
    const t = mount(() => <Probe id={7} kind="show" />, { itemRequest: (id: number) => (calls.push(id), Promise.resolve(ANSWER)) }, summary());
    await t.settle();
    expect(calls).toEqual([]);
    t.advance(16);
    await t.settle();
    expect(calls).toEqual([7]);
    expect(t.text(".item-request")).toEqual(["Approved"]);
  });

  it("isn't asked for where the profile may not ask and has asked for nothing, nor for a season", async () => {
    const calls: number[] = [];
    const t = mount(() => <Probe id={7} kind="show" />, { itemRequest: (id: number) => (calls.push(id), Promise.resolve(ANSWER)) }, summary({ access: "off" }));
    t.advance(16);
    await t.settle();
    expect(calls).toEqual([]);
    const u = mount(() => <Probe id={8} kind="season" />, { itemRequest: (id: number) => (calls.push(id), Promise.resolve(ANSWER)) }, summary());
    u.advance(16);
    await u.settle();
    expect(calls).toEqual([]);
  });

  it("follows a download's progress every 10 s while it moves", async () => {
    let progress = 0.1;
    const answer = (): ItemRequest => ({
      ...ANSWER,
      request: request({ id: 4, kind: "show", tmdb_id: 20, status: "approved", stage: progress < 1 ? "downloading" : "available", download: progress < 1 ? { progress, state: "downloading", eta: null, episodes: 2 } : null }),
    });
    let calls = 0;
    const t = mount(() => <Probe id={7} kind="show" />, { itemRequest: () => (calls++, Promise.resolve(answer())) }, summary());
    t.advance(16);
    await t.settle();
    expect(t.text(".item-request")).toEqual(["Downloading 10%"]);
    progress = 0.6;
    t.advance(10_000);
    await t.settle();
    expect(t.text(".item-request")).toEqual(["Downloading 60%"]);
    progress = 1;
    t.advance(10_000);
    await t.settle();
    expect(t.text(".item-request")).toEqual(["Ready to watch"]);
    const asked = calls;
    t.advance(60_000);
    await t.settle();
    expect(calls).toBe(asked);
  });
});

describe("ready-to-watch news", () => {
  const NEWS = { id: 1, text: "“Arrival” is ready to watch.", itemId: 7, poster: null };

  it("waits for the remote to rest, takes focus, and gives it back on Back", () => {
    const t = mount(
      (_entry, app) => (
        <>
          <F fid="page" initial label="A page" />
          <RequestNotices state={app} free={() => true} onOpen={() => undefined} noticeRef={() => undefined} />
        </>
      ),
      {},
      summary(),
    );
    expect(t.fid()).toBe("page");
    t.press("down");
    act(() => void (t.app.notices.value = [NEWS]));
    t.advance(1000);
    expect(document.querySelector(".request-notice")).toBeNull();
    t.advance(600);
    expect(t.text(".request-notice-text .t-label")).toEqual(["“Arrival” is ready to watch."]);
    expect(t.focus.current?.textContent).toBe("Open");
    t.press("back");
    expect(document.querySelector(".request-notice")).toBeNull();
    expect(t.fid()).toBe("page");
    expect(t.app.notices.value).toEqual([]);
  });

  it("waits while something else holds focus; OK in its first moment does nothing, then Open opens", () => {
    let free = false;
    const opened: number[] = [];
    const t = mount(
      (_entry, app) => (
        <>
          <F fid="page" initial label="A page" />
          <RequestNotices state={app} free={() => free} onOpen={(n) => void opened.push(n.id)} noticeRef={() => undefined} />
        </>
      ),
      {},
      summary(),
    );
    act(() => void (t.app.notices.value = [NEWS]));
    t.advance(5000);
    expect(document.querySelector(".request-notice")).toBeNull();
    free = true;
    t.advance(250);
    expect(document.querySelector(".request-notice")).not.toBeNull();
    // Meant for what had focus before it came.
    t.press("ok");
    expect(opened).toEqual([]);
    t.advance(600);
    t.press("ok");
    expect(opened).toEqual([1]);
    expect(document.querySelector(".request-notice")).toBeNull();
  });

  it("gives focus to what took the place of what had it, else to the page's first", () => {
    const version = signal(0);
    const there = signal(true);
    function Page() {
      return (
        <G kind="row">
          {there.value && <F key={version.value} fid="page" initial label="A page" />}
          <F fid="other" label="Another" />
        </G>
      );
    }
    const t = mount(
      (_entry, app) => (
        <>
          <Page />
          <RequestNotices state={app} free={() => true} onOpen={() => undefined} noticeRef={() => undefined} />
        </>
      ),
      {},
      summary(),
    );
    act(() => void (t.app.notices.value = [NEWS, { ...NEWS, id: 2 }]));
    t.advance(250);
    expect(t.focus.current?.textContent).toBe("Open");
    // The page draws what had focus again while the notice shows.
    act(() => void version.value++);
    t.press("back");
    expect(document.querySelector(".request-notice")).toBeNull();
    expect(t.fid()).toBe("page");
    expect(t.focus.current!.isConnected).toBe(true);
    t.advance(2000);
    expect(t.focus.current?.textContent).toBe("Open");
    // Now it goes for good.
    act(() => void (there.value = false));
    t.press("down");
    expect(document.querySelector(".request-notice")).toBeNull();
    expect(t.fid()).toBe("other");
  });

  it("stays while newer news pushes older out of the queue", () => {
    const t = mount(
      (_entry, app) => (
        <>
          <F fid="page" initial label="A page" />
          <RequestNotices state={app} free={() => true} onOpen={() => undefined} noticeRef={() => undefined} />
        </>
      ),
      {},
      summary(),
    );
    const hear = (id: number) => (t.app as unknown as { handleEvent(e: ServerEvent): void }).handleEvent({ type: "request_available", request_id: id, title: `T${id}`, item_id: id, complete: true });
    act(() => hear(1));
    t.advance(250);
    expect(t.text(".request-notice-text .t-label")).toEqual(["“T1” is ready to watch."]);
    act(() => [2, 3, 4, 5].forEach(hear));
    expect(t.text(".request-notice-text .t-label")).toEqual(["“T1” is ready to watch."]);
    expect(t.focus.current?.textContent).toBe("Open");
    expect(t.app.notices.value.map((n) => n.text)).toEqual(["“T1” is ready to watch.", "“T3” is ready to watch.", "“T4” is ready to watch.", "“T5” is ready to watch."]);
    t.press("back");
    expect(t.fid()).toBe("page");
    expect(t.app.notices.value.length).toBe(3);
  });

  it("goes by itself after 10 s untouched, focus back where it was", () => {
    const t = mount(
      (_entry, app) => (
        <>
          <F fid="page" initial label="A page" />
          <RequestNotices state={app} free={() => true} onOpen={() => undefined} noticeRef={() => undefined} />
        </>
      ),
      {},
      summary(),
    );
    act(() => void (t.app.notices.value = [NEWS]));
    t.advance(250);
    expect(document.querySelector(".request-notice")).not.toBeNull();
    t.advance(10_000);
    expect(document.querySelector(".request-notice")).toBeNull();
    expect(t.fid()).toBe("page");
  });
});
