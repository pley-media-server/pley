import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../src/api/client";
import type { Collection, CollectionCategory, Library, User } from "../../src/api/types";
import type { KeyValue } from "../../src/auth/tokens";
import type { Platform } from "../../src/platform";
import { AppState } from "../../src/state";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import { AppContext } from "../../src/ui/screens/common";
import { LibraryScreen } from "../../src/ui/screens/library";
import type { Entry, LibraryTab } from "../../src/ui/shell/navigator";

/** The Collections tab's focus: never left on something that went. */

const MOVIES = { id: 1, name: "Movies", kind: "movies", item_count: 3, scanning: false } as Library;
const SERVER = "http://pley.test";
const FRANCHISES: CollectionCategory = { id: 2, key: "franchises", title: "Franchises" };
const OTHER: CollectionCategory = { id: 3, key: "other", title: "Other" };

const collection = (id: number, category: CollectionCategory): Collection =>
  ({ id, library_id: 1, title: `Collection ${id}`, item_count: 2, poster_url: "", backdrop_url: "", category }) as unknown as Collection;

function memoryStore(): KeyValue {
  const kept = new Map<string, string>();
  return { get: (k) => kept.get(k) ?? null, set: (k, v) => void kept.set(k, v), remove: (k) => void kept.delete(k) };
}

let detach: (() => void) | null = null;
let root: HTMLElement | null = null;

function setup(collections: Collection[], categories: Promise<{ categories: CollectionCategory[] }>, tab: LibraryTab, state: Record<string, unknown> = {}) {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const platform = { name: "tizen", clientId: "test", storage: memoryStore() } as unknown as Platform;
  const app = new AppState(platform, () => Promise.reject(new Error("no network in tests")));
  const api = {
    server: SERVER,
    libraryItems: () => Promise.resolve({ total: 0, items: [] }),
    libraryFilters: () => Promise.resolve({ genres: [], years: [] }),
    libraryHubs: () => Promise.resolve({ hubs: [] }),
    collections: () => Promise.resolve({ collections }),
    collectionCategories: () => categories,
    prefetch: () => Promise.resolve(),
  };
  (app.api as { value: unknown }).value = api;
  app.server.value = SERVER;
  app.me.value = { id: 1 } as User;
  app.libraries.value = [MOVIES];
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  detach = input.attach(document);
  const ui = {
    focus,
    input,
    nav: { open: () => undefined, pop: () => true },
    api,
    images: { url: (u: string) => u, ready: () => null, isMissing: () => false, is: () => false, load: () => () => undefined },
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
  const entry: Entry = { key: { name: "library", id: 1, tab }, id: 1, memory: null, state };
  act(() =>
    render(
      <AppContext.Provider value={app}>
        <UiContext.Provider value={ui}>
          <LibraryScreen entry={entry} id={1} tab={tab} />
        </UiContext.Provider>
      </AppContext.Provider>,
      root!,
    ),
  );
  const settle = () =>
    act(async () => {
      for (let i = 0; i < 10; i++) await Promise.resolve();
    });
  const el = (fid: string) => root!.querySelector<HTMLElement>(`[data-fid="${fid}"]`);
  const press = (target: HTMLElement) => {
    act(() => void focus.focus(target));
    act(() => input.dispatch("ok", "down", false));
    act(() => input.dispatch("ok", "up", false));
  };
  /** What has focus; null when what had it went. */
  const focused = () => {
    const current = focus.current;
    return current && current.isConnected ? current.getAttribute("data-fid") : null;
  };
  const later = () => act(() => void vi.advanceTimersByTime(20));
  return { settle, el, press, focused, later, focus };
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

describe("the Collections tab's focus", () => {
  it("moves from a placeholder to the first collection when they come", async () => {
    let answer: (v: { categories: CollectionCategory[] }) => void = () => undefined;
    const waiting = new Promise<{ categories: CollectionCategory[] }>((resolve) => (answer = resolve));
    const t = setup([collection(10, OTHER), collection(11, OTHER)], waiting, "library");
    await t.settle();
    // Shown by a tab, not opened on it: its tiles don't claim focus of themselves.
    t.press(t.el("tab/collections")!);
    await t.settle();
    const placeholder = t.el("collections/cell0");
    expect(placeholder).not.toBeNull();
    act(() => void t.focus.focus(placeholder!));
    answer({ categories: [] });
    await t.settle();
    expect(t.focused()).toBe("collections/10");
  });

  it("shows every collection in one grid on a server from before categories", async () => {
    // Its collections name no category, and its GET /api/collection-categories answers 404 "no such endpoint":
    // no categories, the tab as it was.
    const older = (id: number): Collection => {
      const c = collection(id, OTHER) as { category?: CollectionCategory };
      delete c.category;
      return c as Collection;
    };
    const gone = Promise.reject(new ApiError(404, "not_found", "no such endpoint"));
    gone.catch(() => undefined);
    const t = setup([older(10), older(11)], gone, "collections");
    await t.settle();
    expect(t.el("collections/10")).not.toBeNull();
    expect(t.el("collections/11")).not.toBeNull();
    expect(t.el("collections/cell0")).toBeNull();
    // No chips and no headings: one grid, the count alone.
    expect(t.el("chip/category")).toBeNull();
    expect(document.querySelector(".collections-rows")).toBeNull();
    expect(document.querySelector(".collections-count")?.textContent).toBe("2 collections");
    expect(t.focused()).toBe("collections/10");
  });

  it("goes up from the chips to the tabs, not back into the rows", async () => {
    // jsdom lays nothing out: the page as a browser lays it out, the rows' box of no height at the page's top.
    const original = Element.prototype.getBoundingClientRect;
    const at = (x: number, y: number, w: number, h: number) => ({ x, y, width: w, height: h, left: x, top: y, right: x + w, bottom: y + h, toJSON: () => ({}) }) as DOMRect;
    Element.prototype.getBoundingClientRect = function (this: Element) {
      const fid = this.getAttribute("data-fid") ?? "";
      const cls = this.getAttribute("class") ?? "";
      if (cls.indexOf("library-tabs") >= 0) return at(589, 29, 323, 40);
      if (fid.indexOf("tab/") === 0) return at(fid === "tab/collections" ? 806 : fid === "tab/library" ? 724 : 589, 29, 100, 40);
      if (cls.indexOf("library-chips") >= 0) return at(96, 80, 816, 40);
      if (fid === "chip/category") return at(96, 80, 89, 40);
      if (cls.indexOf("page-scroll") >= 0) return at(0, 0, 960, 0);
      if (cls.indexOf("row-track-group") >= 0) return at(0, 174, 960, 210);
      if (fid.indexOf("collections/") === 0) return at(96, 174, 108, 162);
      return at(0, 0, 0, 0);
    };
    try {
      const highlights: CollectionCategory = { id: 1, key: "highlights", title: "Highlights" };
      const t = setup([collection(10, highlights), collection(11, FRANCHISES)], Promise.resolve({ categories: [highlights, FRANCHISES] }), "collections");
      await t.settle();
      expect(t.el("chip/category")).not.toBeNull();
      act(() => void t.focus.focus(t.el("chip/category")!));
      act(() => void t.focus.move("up"));
      expect(t.focused()).toBe("tab/collections");
    } finally {
      Element.prototype.getBoundingClientRect = original;
    }
  });

  it("moves to the collections when clearing the category takes the chips away", async () => {
    // Every collection is of one category, chosen: cleared, there is nothing left to choose.
    const t = setup([collection(10, FRANCHISES), collection(11, FRANCHISES)], Promise.resolve({ categories: [FRANCHISES] }), "library", {
      colCategory: 2,
    });
    await t.settle();
    t.press(t.el("tab/collections")!);
    await t.settle();
    t.press(t.el("chip/clear")!);
    await t.settle();
    t.later();
    expect(t.el("chip/category")).toBeNull();
    expect(t.focused()).toBe("collections/10");
  });
});
