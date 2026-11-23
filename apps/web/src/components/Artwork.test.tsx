import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { markLayers } from "pley-placeholder-mark";
import { Artwork } from "./Artwork";

/** The `d` of the mark's front poster, where the glyph is cut out, or null without a mark. */
function front(container: HTMLElement): string | null {
  return container.querySelector("svg path[fill-rule='evenodd']")?.getAttribute("d") ?? null;
}

describe("Artwork's placeholder", () => {
  it("shows Pley's mark with its glyph once no picture will come", () => {
    const { container } = render(<Artwork candidates={[]} width={160} alt="" title="Northern Lights" mark="shows" />);
    expect(front(container)).toBe(markLayers("shows")[2]!.path);
    expect(container).not.toHaveTextContent("Northern Lights");
  });

  it("shows the mark after every candidate failed, not while one loads", () => {
    const { container } = render(<Artwork candidates={["/a.jpg"]} width={160} alt="" title="Northern Lights" mark="movies" />);
    expect(front(container)).toBeNull();
    fireEvent.error(container.querySelector("img")!);
    expect(front(container)).toBe(markLayers("movies")[2]!.path);
  });

  it("shows nothing on the gradient without a mark: a backdrop, a mosaic's quarter", () => {
    const { container } = render(<Artwork candidates={[]} width={160} alt="" title="Northern Lights" />);
    expect(container.querySelector("svg")).toBeNull();
    expect(container).not.toHaveTextContent("Northern Lights");
  });

  it("shows the title where nothing beside the picture names it", () => {
    const { container } = render(<Artwork candidates={[]} width={160} alt="" title="Northern Lights" mark="play" titled />);
    expect(container).toHaveTextContent("Northern Lights");
    expect(container.querySelector("svg")).toBeNull();
  });
});
