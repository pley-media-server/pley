import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { GRID_ROW_GAP, gridPitch, PosterGrid } from "../../src/ui/kit/grid";

/**
 * A grid under a hero (a collection's page): the cells' box comes after the
 * hero in the content (screens.css), so its slots are placed from under the
 * hero, the first row a row's gap below it. Placed from the content's top,
 * they fell a hero's height further, off the screen.
 */

let root: HTMLElement | null = null;

function grid(heroHeight: number | null): HTMLElement[] {
  root = document.createElement("div");
  document.body.appendChild(root);
  const focus = new FocusManager();
  focus.setPage(root);
  const ui = { focus, claim: () => undefined, scale: 1, tier: "high", reducedMotion: false } as unknown as Ui;
  act(() =>
    render(
      <UiContext.Provider value={ui}>
        <PosterGrid
          kind="grid"
          count={9}
          top={58}
          clipTop={0}
          hero={heroHeight === null ? undefined : <div class="hero" />}
          heroHeight={heroHeight ?? undefined}
          cell={(i) => <div data-f="" data-fid={`cell/${i}`} />}
        />
      </UiContext.Provider>,
      root!,
    ),
  );
  return Array.from(root.querySelectorAll<HTMLElement>(".grid-slot"));
}

afterEach(() => {
  if (root) act(() => render(null, root!));
  root?.remove();
  root = null;
});

describe("a grid's rows", () => {
  it("start a row's gap under the hero, counted from the hero's foot", () => {
    const slots = grid(360);
    const hero = root!.querySelector<HTMLElement>(".grid-hero")!;
    expect(hero.style.height).toBe("360px");
    // The hero comes first in the content, the cells' box after it.
    expect(hero.nextElementSibling?.classList.contains("grid-cells")).toBe(true);
    expect(slots[0]!.style.top).toBe(`${GRID_ROW_GAP}px`);
    // The second row (seven posters across) a row's pitch below.
    expect(slots[7]!.style.top).toBe(`${GRID_ROW_GAP + gridPitch("grid")}px`);
  });

  it("start at the content's top without a hero", () => {
    const slots = grid(null);
    expect(slots[0]!.style.top).toBe("0px");
    expect(slots[7]!.style.top).toBe(`${gridPitch("grid")}px`);
  });
});
