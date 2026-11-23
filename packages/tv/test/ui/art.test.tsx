import { render } from "preact";
import { act } from "preact/test-utils";
import { describe, expect, it } from "vitest";
import { UiContext, type Ui } from "../../src/ui/context";
import { Art } from "../../src/ui/kit/art";

/** Art in a UI whose pictures load as `load` says: `done` is called with the object URL, or null when it can't be had. */
function show(candidates: string[], load: (url: string, done: (objectUrl: string | null) => void) => void) {
  const ui = {
    images: { url: (u: string) => u, ready: () => null, isMissing: () => false, load: (u: string, done: (o: string | null) => void) => (load(u, done), () => undefined) },
    scale: 1,
    tier: "high",
  } as unknown as Ui;
  const root = document.createElement("div");
  act(() =>
    render(
      <UiContext.Provider value={ui}>
        <Art candidates={candidates} width={96} title="Northern Lights" />
      </UiContext.Provider>,
      root,
    ),
  );
  return root;
}

describe("a picture's placeholder", () => {
  it("shows Pley's mark, and not the title, when there is no picture", () => {
    const root = show([], () => undefined);
    expect(root.querySelector(".tile-placeholder .tile-mark")).not.toBeNull();
    expect(root.textContent).toBe("");
  });

  it("shows the mark once every address has failed", () => {
    const root = show(["/a", "/b"], (_u, done) => done(null));
    expect(root.querySelector(".tile-mark")).not.toBeNull();
  });

  it("is the gradient alone while a picture loads", () => {
    const root = show(["/a"], () => undefined);
    expect(root.querySelector(".tile-placeholder")).not.toBeNull();
    expect(root.querySelector(".tile-mark")).toBeNull();
    expect(root.textContent).toBe("");
  });

  it("has no mark under a picture that came", () => {
    const root = show(["/a"], (_u, done) => done("blob:a"));
    expect(root.querySelector("img")).not.toBeNull();
    expect(root.querySelector(".tile-mark")).toBeNull();
  });
});
