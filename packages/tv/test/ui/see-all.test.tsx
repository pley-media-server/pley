import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Hub, ItemSummary, Library, LibrarySort, User } from "../../src/api/types";
import type { KeyValue } from "../../src/auth/tokens";
import type { Platform } from "../../src/platform";
import { AppState } from "../../src/state";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import { AppContext } from "../../src/ui/screens/common";
import { LibraryScreen } from "../../src/ui/screens/library";
import { ParamsStore } from "../../src/ui/screens/library-data";
import type { Entry, LibraryTab } from "../../src/ui/shell/navigator";

/** A row's See all: the library's Library tab in the row's order, not kept until the view is changed there. */

function memoryStore(): KeyValue {
  const kept = new Map<string, string>();
  return {
    get: (k) => kept.get(k) ?? null,
    set: (k, v) => void kept.set(k, v),
    remove: (k) => void kept.delete(k),
  };
}

const MOVIES = { id: 1, name: "Movies", kind: "movies", item_count: 2, scanning: false } as Library;
const SERVER = "http://pley.test";

const item = (id: number): ItemSummary => ({ id, kind: "movie", title: `Film ${id}`, user_state: { watched: false, position_ms: 0, play_count: 0 } }) as ItemSummary;

let detach: (() => void) | null = null;
let root: HTMLElement | null = null;

function setup(opts: { tab?: LibraryTab; sort?: LibrarySort; store?: KeyValue }) {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const storage = opts.store ?? memoryStore();
  const platform = { name: "tizen", clientId: "test", storage } as unknown as Platform;
  const app = new AppState(platform, () => Promise.reject(new Error("no network in tests")));
  const asked: { sort?: string; order?: string; watched?: boolean }[] = [];
  const hubs: Hub[] = [{ key: "recently_added:1", title: "Recently Added", library_id: 1, items: [item(1), item(2)] }];
  const api = {
    server: SERVER,
    libraryItems: (_id: number, q: { sort?: string; order?: string; watched?: boolean }) => {
      asked.push({ sort: q.sort, order: q.order, watched: q.watched });
      return Promise.resolve({ total: 2, items: [item(1), item(2)] });
    },
    libraryFilters: () => Promise.resolve({ genres: [], years: [] }),
    libraryHubs: () => Promise.resolve({ hubs }),
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
  const ui = {
    focus,
    input,
    nav: { open: () => undefined, pop: () => true },
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
  const entry: Entry = { key: { name: "library", id: 1, tab: opts.tab, sort: opts.sort }, id: 1, memory: null, state: {} };
  act(() =>
    render(
      <AppContext.Provider value={app}>
        <UiContext.Provider value={ui}>
          <LibraryScreen entry={entry} id={1} tab={opts.tab} sort={opts.sort} />
        </UiContext.Provider>
      </AppContext.Provider>,
      root!,
    ),
  );
  const settle = () =>
    act(async () => {
      for (let i = 0; i < 10; i++) await Promise.resolve();
    });
  const select = (fid: string) => {
    const el = root!.querySelector<HTMLElement>(`[data-fid="${fid}"]`);
    if (!el) throw new Error(`no ${fid}`);
    act(() => void focus.focus(el));
    act(() => input.dispatch("ok", "down", false));
    act(() => input.dispatch("ok", "up", false));
  };
  return { asked, settle, select, storage, entry };
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

describe("See all", () => {
  it("from Home lists the library in the row's order, unfiltered, and keeps the view it was left in", async () => {
    const store = memoryStore();
    const left = { sort: "title", order: "asc", watched: false, genre: null, year: null, resolution: null, dynamicRange: null } as const;
    new ParamsStore(store, SERVER, 1).set(1, left);
    const t = setup({ tab: "library", sort: "added", store });
    await t.settle();
    expect(t.asked[0]).toEqual({ sort: "added", order: "desc", watched: undefined });
    // The view left before is still the one a library opened from the rail shows.
    expect(new ParamsStore(store, SERVER, 1).get(1, "movies")).toEqual(left);
  });

  it("from a library's Recommended row shows its Library tab in that row's order", async () => {
    const t = setup({ tab: "recommended" });
    await t.settle();
    expect(t.asked).toEqual([]);
    t.select("recently_added:1/all");
    await t.settle();
    expect(t.asked[0]).toEqual({ sort: "added", order: "desc", watched: undefined });
    expect(t.entry.state.tab).toBe("library");
  });
});
