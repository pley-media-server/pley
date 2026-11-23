import { render } from "preact";
import { act } from "preact/test-utils";
import { markLayers, type MarkGlyph } from "pley-placeholder";
import { describe, expect, it } from "vitest";
import { UiContext, type Ui } from "../../src/ui/context";
import { Art } from "../../src/ui/kit/art";

/** Art in a UI whose pictures load as `load` says: `done` is called with the object URL, or null when it can't be had. */
function show(candidates: string[], load: (url: string, done: (objectUrl: string | null) => void) => void, mark: MarkGlyph | null = "movies") {
  const ui = {
    images: { url: (u: string) => u, ready: () => null, isMissing: () => false, load: (u: string, done: (o: string | null) => void) => (load(u, done), () => undefined) },
    scale: 1,
    tier: "high",
  } as unknown as Ui;
  const root = document.createElement("div");
  act(() =>
    render(
      <UiContext.Provider value={ui}>
        <Art candidates={candidates} width={96} title="Northern Lights" mark={mark ?? undefined} />
      </UiContext.Provider>,
      root,
    ),
  );
  return root;
}

/** The `d` of the mark's front poster, where its glyph is cut out. */
function front(root: HTMLElement): string | null {
  return root.querySelector(".tile-mark path[fill-rule='evenodd']")?.getAttribute("d") ?? null;
}

describe("a picture's placeholder", () => {
  it("shows Pley's mark with its glyph, and not the title, when there is no picture", () => {
    const root = show([], () => undefined);
    expect(root.querySelector(".tile-placeholder .tile-mark")).not.toBeNull();
    expect(front(root)).toBe(markLayers("movies")[2]!.path);
    expect(root.textContent).toBe("");
  });

  it("shows the mark once every address has failed", () => {
    const root = show(["/a", "/b"], (_u, done) => done(null), "music");
    expect(front(root)).toBe(markLayers("music")[2]!.path);
  });

  it("is the gradient alone while a picture loads", () => {
    const root = show(["/a"], () => undefined);
    expect(root.querySelector(".tile-placeholder")).not.toBeNull();
    expect(root.querySelector(".tile-mark")).toBeNull();
    expect(root.textContent).toBe("");
  });

  it("is the gradient alone without a glyph: a backdrop, a person, faint art", () => {
    const root = show([], () => undefined, null);
    expect(root.querySelector(".tile-placeholder")).not.toBeNull();
    expect(root.querySelector(".tile-mark")).toBeNull();
  });

  it("has no mark under a picture that came", () => {
    const root = show(["/a"], (_u, done) => done("blob:a"));
    expect(root.querySelector("img")).not.toBeNull();
    expect(root.querySelector(".tile-mark")).toBeNull();
  });
});
