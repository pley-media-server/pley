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

function setup(entries: RailEntries) {
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
  const draw = (e: RailEntries) =>
    act(() =>
      render(
        <UiContext.Provider value={ui}>
          <Rail entries={e} me={null} current="home" railRef={(a) => (api = a)} />
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
  /** Whether the entry is drawn where it was laid out: neither it nor anything between it and the rail is moved. */
  const still = (label: string) => {
    const rail = root!.querySelector<HTMLElement>(".rail")!;
    let el: HTMLElement | null = root!.querySelector<HTMLElement>(`[aria-label="${label}"]`);
    if (!el) throw new Error(`no entry ${label}`);
    for (; el && el !== rail; el = el.parentElement) if (el.style.transform) return false;
    return true;
  };
  return { focus, api: () => api!, draw, scroll, move, focused, still };
}

afterEach(() => {
  if (root) act(() => render(null, root!));
  root = null;
});

describe("the rail's entries", () => {
  it("scroll with focus, showing the next entry the way it goes, Settings and the avatar staying put", () => {
    // Search, Home, Live TV, six libraries, Libraries, Playlists, Now Playing: twelve, nine of which fit.
    const t = setup(railEntries(libraries(8), true, true));
    expect(t.api().focusCurrent()).toBe(true);
    expect(t.focused()).toBe("Home");
    expect(t.scroll()).toBe(0);
    t.move("down", 6);
    expect(t.focused()).toBe("Library 5");
    expect(t.scroll()).toBe(0);
    t.move("down");
    // Library 6 at the foot, and Libraries after it.
    expect(t.focused()).toBe("Library 6");
    expect(t.scroll()).toBe(34);
    t.move("down", 3);
    expect(t.focused()).toBe("Now Playing");
    expect(t.scroll()).toBe(122);
    t.move("down");
    expect(t.focused()).toBe("Settings");
    expect(t.scroll()).toBe(122);
    // Scrolled: the entries moved, the avatar and Settings where they were.
    expect([t.still("Home"), t.still("Switch profile"), t.still("Settings")]).toEqual([false, true, true]);
    t.move("up");
    expect(t.focused()).toBe("Now Playing");
    t.move("up", 9);
    expect(t.focused()).toBe("Live TV");
    expect(t.scroll()).toBe(44);
    t.move("up", 2);
    expect(t.focused()).toBe("Search");
    expect(t.scroll()).toBe(0);
    t.move("up");
    expect(t.focused()).toBe("Switch profile");
    expect(t.scroll()).toBe(0);
    expect([t.still("Home"), t.still("Switch profile"), t.still("Settings")]).toEqual([true, true, true]);
  });

  it("show the next one coming on the move after one above the focused entry went", () => {
    const t = setup(railEntries(libraries(8), true, true));
    // The rail opened on Library 6 (its page's): it alone brought into view, Libraries after it cut off.
    const sixth = root!.querySelector<HTMLElement>('[data-rail="library:6"]')!.closest<HTMLElement>("[data-f]")!;
    act(() => void t.focus.focus(sixth));
    expect(t.scroll()).toBe(0);
    // Live TV goes (the server's channels reloaded): Library 6 moves up a place.
    t.draw(railEntries(libraries(8), true, false));
    expect(t.focused()).toBe("Library 6");
    expect(t.scroll()).toBe(0);
    // Down to Libraries brings Playlists after it into view, as any move down does.
    t.move("down");
    expect(t.focused()).toBe("Libraries");
    expect(t.scroll()).toBe(34);
  });

  it("keep the focused entry in view when they change, and the scroll within their ends", () => {
    const t = setup(railEntries(libraries(8), true, true));
    t.api().focusCurrent();
    t.move("down", 11);
    expect(t.focused()).toBe("Settings");
    t.move("up");
    expect(t.scroll()).toBe(122);
    // Now Playing goes: eleven entries, Settings focused, the last one ending above it.
    t.move("down");
    t.draw(railEntries(libraries(8), false, true));
    expect(t.scroll()).toBe(78);
    // Live TV and two libraries go too: ten.
    t.draw(railEntries(libraries(6)));
    expect(t.scroll()).toBe(34);
    // Fewer than fit: no scroll at all.
    t.draw(railEntries(libraries(2)));
    expect(t.scroll()).toBe(0);
  });

  it("bring the focused entry into view when the rail opens on it", () => {
    const t = setup(railEntries(libraries(8), true, true));
    // The rail closed; focus put on its last entry (as on opening for the page it opened).
    const now = root!.querySelector<HTMLElement>('[data-rail="now-playing"]')!.closest<HTMLElement>("[data-f]")!;
    act(() => void t.focus.focus(now));
    expect(t.scroll()).toBe(122);
  });

  it("don't scroll while nine or fewer", () => {
    const t = setup(railEntries(libraries(5)));
    t.api().focusCurrent();
    t.move("down", 7);
    expect(t.focused()).toBe("Collections");
    expect(t.scroll()).toBe(0);
  });
});
