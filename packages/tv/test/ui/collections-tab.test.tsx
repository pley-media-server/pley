import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
