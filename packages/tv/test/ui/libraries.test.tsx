import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../src/api/client";
import type { Library, User } from "../../src/api/types";
import type { KeyValue } from "../../src/auth/tokens";
import type { Platform } from "../../src/platform";
import { AppState } from "../../src/state";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import type { Key } from "../../src/ui/keys";
import { AppContext } from "../../src/ui/screens/common";
import { LibrariesScreen, SORT_NEEDS_NEWER } from "../../src/ui/screens/library";
import { gridStep, inOrder, moved, sameOrder } from "../../src/ui/screens/library-data";
import type { Entry, PageKey } from "../../src/ui/shell/navigator";

/** Sorting the libraries on the Libraries page: lifted by OK, moved by the arrows, dropped by OK, saved for the profile. */

function memoryStore(): KeyValue {
  const kept = new Map<string, string>();
  return {
    get: (k) => kept.get(k) ?? null,
    set: (k, v) => void kept.set(k, v),
    remove: (k) => void kept.delete(k),
  };
}

const lib = (id: number, name: string): Library => ({ id, name, kind: "movies", item_count: 10, scanning: false }) as Library;
const LIBRARIES = [lib(1, "Movies"), lib(2, "Shows"), lib(3, "Music"), lib(4, "Kids"), lib(5, "Docs")];

let detach: (() => void) | null = null;
let root: HTMLElement | null = null;
/** Each test its own server: what one learns of its server (too old to sort) stays with it. */
let servers = 0;

function setup(opts: { sort?: boolean; answer?: (ids: number[] | null) => Promise<Library[]> } = {}) {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const platform = { name: "tizen", clientId: "test", storage: memoryStore() } as unknown as Platform;
  const app = new AppState(platform, () => Promise.reject(new Error("no network in tests")));
  const sent: (number[] | null)[] = [];
  const answer = opts.answer ?? ((ids: number[] | null) => Promise.resolve(ids ? inOrder(LIBRARIES, ids) : LIBRARIES));
  let homes = 0;
  const api = {
    sortLibraries: (ids: number[]) => {
      sent.push(ids);
      return answer(ids);
    },
    forgetLibraryOrder: () => {
      sent.push(null);
      return answer(null);
    },
    home: () => {
      homes++;
      return Promise.resolve({ hubs: [] });
    },
    me: () => Promise.resolve({ id: 1 }),
    libraries: () => Promise.resolve(LIBRARIES),
    profiles: () => Promise.resolve([]),
  };
  (app.api as { value: unknown }).value = api;
  app.server.value = `http://pley${++servers}.test`;
  app.me.value = { id: 1 } as User;
  app.libraries.value = LIBRARIES;
  Object.defineProperty(app, "tokenStore", { get: () => ({ scopes: () => ["library", "play", "playlists"] }) });
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  detach = input.attach(document);
  const toasts: string[] = [];
  const opened: PageKey[] = [];
  let popped = 0;
  const ui = {
    focus,
    input,
    nav: {
      open: (k: PageKey) => opened.push(k),
      pop: () => {
        popped++;
        return true;
      },
    },
    api,
    toast: (t: string) => toasts.push(t),
    claim: (el: HTMLElement, _fid: string | null, initial: boolean) => {
      const current = focus.current;
      if (initial && focus.inScope(el) && !(current && current.isConnected && focus.inScope(current))) focus.focus(el);
    },
    toRail: () => false,
    scale: 1,
    tier: "high",
    reducedMotion: false,
  } as unknown as Ui;
  const entry: Entry = { key: { name: "libraries", sort: opts.sort }, id: 1, memory: null, state: {} };
  act(() =>
    render(
      <AppContext.Provider value={app}>
        <UiContext.Provider value={ui}>
          <LibrariesScreen entry={entry} sort={opts.sort} />
        </UiContext.Provider>
      </AppContext.Provider>,
      root!,
    ),
  );
  const press = (key: Key) => {
    act(() => input.dispatch(key, "down", false));
    act(() => input.dispatch(key, "up", false));
    act(() => void vi.advanceTimersByTime(150));
  };
  const order = () => Array.from(root!.querySelectorAll(".library-card")).map((e) => Number(e.getAttribute("data-fid")!.split("/")[1]));
  const el = (fid: string) => root!.querySelector<HTMLElement>(`[data-fid="${fid}"]`);
  const select = (fid: string) => {
    act(() => void focus.focus(el(fid)));
    press("ok");
  };
  const title = () => root!.querySelector(".libraries-header .t-page")!.textContent;
  const lifted = () => root!.querySelector(".library-card.lifted")?.getAttribute("data-fid") ?? null;
  const fid = () => focus.current?.getAttribute("data-fid") ?? null;
  /** A side panel's rows, by their words. */
  const rows = () => Array.from(document.querySelectorAll(".side-panel .panel-row")).map((r) => r.textContent);
  const row = (label: string) => Array.from(document.querySelectorAll<HTMLElement>(".side-panel .panel-row")).filter((r) => r.textContent === label)[0] ?? null;
  /** The answers on their way (one change at a time, then Home again). */
  const settle = () =>
    act(async () => {
      for (let i = 0; i < 10; i++) await Promise.resolve();
    });
  return { app, input, focus, sent, toasts, opened, popped: () => popped, homes: () => homes, press, order, el, select, title, lifted, fid, rows, row, settle };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(1_000_000);
});
afterEach(() => {
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root = null;
  vi.useRealTimers();
});

describe("the libraries' own order", () => {
  it("lists them in an order, the ones it doesn't name after, as they came", () => {
    expect(inOrder(LIBRARIES, [3, 1]).map((l) => l.id)).toEqual([3, 1, 2, 4, 5]);
    expect(inOrder(LIBRARIES, null)).toBe(LIBRARIES);
    expect(moved([1, 2, 3, 4], 0, 2)).toEqual([2, 3, 1, 4]);
    expect(moved([1, 2, 3, 4], 3, 1)).toEqual([1, 4, 2, 3]);
    expect(sameOrder([1, 2], [1, 2])).toBe(true);
    expect(sameOrder([1, 2], [2, 1])).toBe(false);
  });

  it("moves a card through a grid: along, on to the next row, a row up or down, onto a short last row's end", () => {
    // 3 across, 5 cards: 0 1 2 / 3 4.
    expect(gridStep(2, 1, 5, 3)).toBe(3);
    expect(gridStep(3, -1, 5, 3)).toBe(2);
    expect(gridStep(0, -1, 5, 3)).toBe(-1);
    expect(gridStep(4, 1, 5, 3)).toBe(-1);
    expect(gridStep(1, 3, 5, 3)).toBe(4);
    expect(gridStep(2, 3, 5, 3)).toBe(4);
    expect(gridStep(4, 3, 5, 3)).toBe(-1);
    expect(gridStep(1, -3, 5, 3)).toBe(-1);
    expect(gridStep(4, -3, 5, 3)).toBe(1);
  });
});

describe("Sort libraries", () => {
  it("lifts a card by OK; the arrows move it through the grid, and OK drops it there and saves the order", async () => {
    const t = setup();
    expect(t.title()).toBe("Libraries");
    expect(t.fid()).toBe("library/1");
    t.select("sort");
    expect(t.title()).toBe("Sort libraries");
    // Ready to lift: the first card.
    expect(t.fid()).toBe("library/1");
    t.press("ok");
    expect(t.lifted()).toBe("library/1");
    t.press("right");
    expect(t.order()).toEqual([2, 1, 3, 4, 5]);
    t.press("down");
    expect(t.order()).toEqual([2, 3, 4, 5, 1]);
    // It can't go further down or right: it stays.
    t.press("down");
    t.press("right");
    expect(t.order()).toEqual([2, 3, 4, 5, 1]);
    expect(t.fid()).toBe("library/1");
    expect(t.sent).toEqual([]);
    t.press("ok");
    expect(t.lifted()).toBeNull();
    await t.settle();
    expect(t.sent).toEqual([[2, 3, 4, 5, 1]]);
    // The server's answer is the list now, kept for the next start, and Home asks again for its rows.
    expect(t.app.libraries.value!.map((l) => l.id)).toEqual([2, 3, 4, 5, 1]);
    expect(t.order()).toEqual([2, 3, 4, 5, 1]);
    expect(t.homes()).toBe(1);
    // Still sorting: OK lifts the next one.
    expect(t.title()).toBe("Sort libraries");
  });

  it("puts a lifted card back with Back, and a second Back ends sorting", () => {
    const t = setup();
    t.select("sort");
    t.press("ok");
    t.press("right");
    t.press("right");
    expect(t.order()).toEqual([2, 3, 1, 4, 5]);
    t.press("back");
    expect(t.lifted()).toBeNull();
    expect(t.order()).toEqual([1, 2, 3, 4, 5]);
    expect(t.title()).toBe("Sort libraries");
    t.press("back");
    expect(t.title()).toBe("Libraries");
    expect(t.sent).toEqual([]);
  });

  it("sends nothing for a card dropped where it was lifted", () => {
    const t = setup();
    t.select("sort");
    t.press("ok");
    t.press("right");
    t.press("left");
    t.press("ok");
    expect(t.order()).toEqual([1, 2, 3, 4, 5]);
    expect(t.sent).toEqual([]);
  });

  it("lifts a card from its actions (holding OK): Move", async () => {
    const t = setup();
    act(() => void t.focus.focus(t.el("library/2")));
    act(() => t.input.dispatch("ok", "down", false));
    act(() => void vi.advanceTimersByTime(600));
    act(() => t.input.dispatch("ok", "up", false));
    act(() => void vi.advanceTimersByTime(50));
    // The panel: Open first, then Move and the server's order.
    expect(t.rows()).toEqual(["Open", "Move", "Use the server's order"]);
    expect(t.focus.current?.textContent).toBe("Open");
    act(() => void t.focus.focus(t.row("Move")));
    t.press("ok");
    expect(document.querySelector(".side-panel")).toBeNull();
    expect(t.title()).toBe("Sort libraries");
    expect(t.lifted()).toBe("library/2");
    expect(t.fid()).toBe("library/2");
    t.press("left");
    t.press("ok");
    await t.settle();
    expect(t.sent).toEqual([[2, 1, 3, 4, 5]]);
  });

  it("goes back to the server's order", async () => {
    const t = setup();
    act(() => void (t.app.libraries.value = inOrder(LIBRARIES, [3, 1, 2])));
    expect(t.order()).toEqual([3, 1, 2, 4, 5]);
    t.select("sort");
    t.select("serverOrder");
    await t.settle();
    expect(t.sent).toEqual([null]);
    expect(t.order()).toEqual([1, 2, 3, 4, 5]);
    expect(t.fid()).toBe("serverOrder");
  });

  it("says a server too old to sort needs updating, puts the cards back and offers Sort no more", async () => {
    const t = setup({ answer: () => Promise.reject(new ApiError(404, "not_found", "not found")) });
    t.select("sort");
    t.press("ok");
    t.press("right");
    t.press("ok");
    await t.settle();
    expect(t.toasts).toEqual([SORT_NEEDS_NEWER]);
    expect(t.order()).toEqual([1, 2, 3, 4, 5]);
    expect(t.title()).toBe("Libraries");
    expect(t.el("sort")).toBeNull();
  });

  it("opened by Settings, sorts at once and goes back to it when done", () => {
    const t = setup({ sort: true });
    expect(t.title()).toBe("Sort libraries");
    expect(t.fid()).toBe("library/1");
    t.select("sort");
    expect(t.popped()).toBe(1);
  });

  it("with a card lifted, the pointer carries it to the place it points at, and its click drops it there", async () => {
    const t = setup();
    t.select("sort");
    t.press("ok");
    const targets = Array.from(document.querySelectorAll<HTMLElement>(".library-target"));
    // Two rows of three places, the empty one past the last card leading to the end.
    expect(targets.length).toBe(6);
    act(() => void targets[4]!.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: 500, clientY: 200 })));
    expect(t.order()).toEqual([2, 3, 4, 5, 1]);
    act(() => void targets[5]!.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: 700, clientY: 200 })));
    expect(t.order()).toEqual([2, 3, 4, 5, 1]);
    act(() => void targets[2]!.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: 700, clientY: 100 })));
    expect(t.order()).toEqual([2, 3, 1, 4, 5]);
    // Focus stayed on the lifted card.
    expect(t.fid()).toBe("library/1");
    // A click well after the OK that lifted it (not that press's own click, which webOS may send too).
    act(() => void vi.advanceTimersByTime(500));
    act(() => void targets[2]!.dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: 700, clientY: 100 })));
    expect(t.lifted()).toBeNull();
    await t.settle();
    expect(t.sent).toEqual([[2, 3, 1, 4, 5]]);
  });
});
