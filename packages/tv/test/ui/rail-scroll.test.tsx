import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import type { Library } from "../../src/api/types";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Rail, railEntries, type RailApi, type RailEntries } from "../../src/ui/shell/rail";

/** The rail's entries scrolling between the avatar and Settings, as focus moves along them. */

const libraries = (n: number): Library[] => Array.from({ length: n }, (_, i) => ({ id: i + 1, name: `Library ${i + 1}`, kind: "movies" }) as Library);

let root: HTMLElement | null = null;

function setup(entries: RailEntries, current = "home") {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const focus = new FocusManager();
  focus.setPage(root);
  const ui = {
    focus,
    input: { pointerMoved: false },
    claim: () => undefined,
    nav: { stack: { value: [] }, restart: () => undefined },
  } as unknown as Ui;
  let api: RailApi | null = null;
  const draw = (e: RailEntries, page = current) =>
    act(() =>
      render(
        <UiContext.Provider value={ui}>
          <Rail entries={e} me={null} current={page} railRef={(a) => (api = a)} />
        </UiContext.Provider>,
        root!,
      ),
    );
  draw(entries);
  /** How far the entries are scrolled, from their transform. */
  const scroll = () => {
    const t = root!.querySelector<HTMLElement>(".rail-scroll")!.style.transform;
    const m = /translateY\((-?[\d.]+)px\)/.exec(t);
    return m ? -Number(m[1]) : 0;
  };
  const move = (dir: "up" | "down", times = 1) => {
    for (let i = 0; i < times; i++) act(() => void focus.move(dir));
  };
  const focused = () => focus.current?.getAttribute("aria-label") ?? focus.current?.textContent ?? null;
  /** The entry `id`'s focusable element. */
  const entry = (id: string) => root!.querySelector<HTMLElement>(`[data-rail="${id}"]`)!.closest<HTMLElement>("[data-f]")!;
  /** Whether the entry is drawn where it was laid out: neither it nor anything between it and the rail is moved. */
  const still = (label: string) => {
    const rail = root!.querySelector<HTMLElement>(".rail")!;
    let el: HTMLElement | null = root!.querySelector<HTMLElement>(`[aria-label="${label}"]`);
    if (!el) throw new Error(`no entry ${label}`);
    for (; el && el !== rail; el = el.parentElement) if (el.style.transform) return false;
    return true;
  };
  return { focus, api: () => api!, draw, scroll, move, focused, entry, still };
}

afterEach(() => {
  if (root) act(() => render(null, root!));
  root = null;
});

/**
 * Search, Home, Live TV, six libraries, Libraries, Playlists, Now Playing:
 * twelve, nine of which fit. The scroll with focus on each: down to the
 * fourth place, then the entries move under it until the last nine show.
 */
const twelve = () => railEntries(libraries(8), true, true);
const RULE_12 = [0, 0, 0, 0, 44, 88, 132, 132, 132, 132, 132, 132];

describe("the rail's entries", () => {
  it("scroll under focus fourth from the top, the same both ways, Settings and the avatar staying put", () => {
    const t = setup(twelve());
    expect(t.api().focusCurrent()).toBe(true);
    expect([t.focused(), t.scroll()]).toEqual(["Home", 0]);
    const down: number[] = [];
    for (let i = 2; i < 12; i++) {
      t.move("down");
      down.push(t.scroll());
    }
    expect(down).toEqual(RULE_12.slice(2));
    expect(t.focused()).toBe("Now Playing");
    // Settings moves nothing.
    t.move("down");
    expect([t.focused(), t.scroll()]).toEqual(["Settings", 132]);
    // Scrolled: the entries moved, the avatar and Settings where they were.
    expect([t.still("Home"), t.still("Switch profile"), t.still("Settings")]).toEqual([false, true, true]);
    // Back up: each entry where it was on the way down.
    const up: number[] = [];
    for (let i = 11; i >= 0; i--) {
      t.move("up");
      up.push(t.scroll());
    }
    expect(up).toEqual([...RULE_12].reverse());
    expect(t.focused()).toBe("Search");
    t.move("up");
    expect([t.focused(), t.scroll()]).toEqual(["Switch profile", 0]);
    expect([t.still("Home"), t.still("Switch profile"), t.still("Settings")]).toEqual([true, true, true]);
  });

  it("place the page's entry by the same rule while the rail is closed, so opening it moves nothing", () => {
    const t = setup(twelve(), "library:3");
    expect(t.scroll()).toBe(88);
    t.draw(twelve(), "now-playing");
    expect(t.scroll()).toBe(132);
    t.draw(twelve(), "live");
    expect(t.scroll()).toBe(0);
    t.draw(twelve(), "library:4");
    expect(t.scroll()).toBe(132);
    expect(t.api().focusCurrent()).toBe(true);
    expect([t.focused(), t.scroll()]).toEqual(["Library 4", 132]);
  });

  it("keep the scroll for a page without an entry among them, within their ends", () => {
    const t = setup(twelve(), "now-playing");
    expect(t.scroll()).toBe(132);
    t.draw(twelve(), "settings");
    expect(t.scroll()).toBe(132);
    // Ten entries: the last nine show at 44.
    t.draw(railEntries(libraries(6)), "settings");
    expect(t.scroll()).toBe(44);
  });

  it("place the focused entry by the same rule when they change", () => {
    const t = setup(twelve());
    // Library 6, the ninth of twelve: fourth from the top.
    act(() => void t.focus.focus(t.entry("library:6")));
    expect(t.scroll()).toBe(132);
    // Live TV goes (the server's channels reloaded): the eighth of eleven, the last nine of which show.
    t.draw(railEntries(libraries(8), true, false));
    expect([t.focused(), t.scroll()]).toEqual(["Library 6", 88]);
    // Now Playing and two libraries go too: the eighth of ten.
    t.draw(railEntries(libraries(6)));
    expect([t.focused(), t.scroll()]).toEqual(["Library 6", 44]);
    t.move("down");
    expect([t.focused(), t.scroll()]).toEqual(["Playlists", 44]);
  });

  it("keep the scroll within their ends while Settings has focus and they change", () => {
    const t = setup(twelve());
    t.api().focusCurrent();
    t.move("down", 11);
    expect([t.focused(), t.scroll()]).toEqual(["Settings", 132]);
    // Now Playing goes: eleven entries.
    t.draw(railEntries(libraries(8), false, true));
    expect(t.scroll()).toBe(88);
    // Live TV and two libraries go too: ten.
    t.draw(railEntries(libraries(6)));
    expect(t.scroll()).toBe(44);
    // Fewer than fit: no scroll at all.
    t.draw(railEntries(libraries(2)));
    expect(t.scroll()).toBe(0);
  });

  it("move under the pointer only as far as brings a cut-off entry in whole", () => {
    const t = setup(twelve());
    expect(t.scroll()).toBe(0);
    // Libraries, the tenth, is cut off above Settings: it comes in whole, and no further.
    act(() => t.api().pointTo(t.entry("libraries")));
    expect([t.focused(), t.scroll()]).toEqual(["Libraries", 34]);
    // An entry in view: nothing moves.
    act(() => t.api().pointTo(t.entry("library:2")));
    expect([t.focused(), t.scroll()]).toEqual(["Library 2", 34]);
    // A key from there places the entry it reaches by the rule.
    t.move("down");
    expect([t.focused(), t.scroll()]).toEqual(["Library 3", 88]);
  });

  it("don't scroll while nine or fewer", () => {
    const t = setup(railEntries(libraries(5)));
    t.api().focusCurrent();
    t.move("down", 7);
    expect(t.focused()).toBe("Collections");
    expect(t.scroll()).toBe(0);
  });
});
