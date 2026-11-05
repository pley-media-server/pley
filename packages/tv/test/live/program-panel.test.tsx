import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import type { LiveChannel, LiveProgram } from "../../src/api/types";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import type { Key } from "../../src/ui/keys";
import { ProgramPanel } from "../../src/ui/screens/livetv";

/** A programme's details: Watch, and the favourite beside it as a star alone (Android TV's ProgramPanel). */

const NOW = 1_800_000_000;

let root: HTMLElement | null = null;

function setup(favorite: boolean) {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  const ui = {
    focus,
    input,
    scale: 2,
    claim: (el: HTMLElement, _fid: string | null, initial: boolean) => {
      if (initial && focus.inScope(el)) focus.focus(el);
    },
  } as unknown as Ui;
  const channel: LiveChannel = { id: 1, source_id: 1, name: "Ch 1", number: "1", group: "", logo: null, favorite, has_guide: true, now: null, next: null };
  const program: LiveProgram = {
    channel_id: 1,
    start: NOW - 600,
    stop: NOW + 1200,
    title: "A",
    subtitle: null,
    description: null,
    categories: [],
    image: null,
    season: null,
    episode: null,
    is_new: false,
    is_live: false,
    rating: null,
  };
  const done: string[] = [];
  act(() =>
    render(
      <UiContext.Provider value={ui}>
        <ProgramPanel
          channel={channel}
          category={{ kind: "all" }}
          program={program}
          now={NOW}
          hours24
          onClose={() => done.push("close")}
          onWatch={() => done.push("watch")}
          onFavorite={() => done.push("favourite")}
        />
      </UiContext.Provider>,
      root!,
    ),
  );
  const press = (key: Key) => {
    act(() => input.dispatch(key, "down", false));
    act(() => input.dispatch(key, "up", false));
  };
  return { root: root!, focus, done, press };
}

afterEach(() => {
  if (root) act(() => render(null, root!));
  root = null;
});

describe("a programme's details", () => {
  it("have Watch and the star side by side, in one row", () => {
    const { root } = setup(false);
    const row = root.querySelector(".live-panel-buttons")!;
    expect(row.getAttribute("data-fg")).toBe("row");
    expect(Array.from(row.querySelectorAll("[data-f]")).map((b) => b.className)).toEqual(["pill t-label", "round off-art"]);
    expect(row.querySelector(".pill")!.textContent).toBe("Watch now");
  });

  it("show the star alone, focused too: its words are only heard", () => {
    const { root, focus, press, done } = setup(true);
    expect(focus.current?.textContent).toBe("Watch now");
    press("right");
    const star = root.querySelector<HTMLElement>(".round")!;
    expect(focus.current).toBe(star);
    expect(star.getAttribute("aria-label")).toBe("Remove from favourites");
    expect(root.querySelector(".round-label")).toBeNull();
    expect(root.textContent).not.toContain("favourites");
    expect(star.querySelector("svg")!.getAttribute("fill")).toBe("currentColor");
    press("ok");
    expect(done).toEqual(["favourite"]);
  });

  it("draw the star outlined for a channel that isn't a favourite", () => {
    const { root } = setup(false);
    const star = root.querySelector(".round")!;
    expect(star.getAttribute("aria-label")).toBe("Add to favourites");
    expect(star.querySelector("svg")!.getAttribute("fill")).toBe("none");
  });
});
