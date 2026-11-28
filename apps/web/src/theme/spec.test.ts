import { describe, expect, it } from "vitest";
import css from "../index.css?raw";
import written from "../../../../packages/themes/themes.json?raw";
import { ACCENTS, THEMES } from ".";
import { colorOf, themeSpec } from "./spec";

const spec = themeSpec(css, THEMES, ACCENTS);

describe("the themes every app draws", () => {
  it("are written to packages/themes/themes.json as index.css has them (bun run themes)", () => {
    expect(JSON.parse(written)).toEqual(spec);
  });

  it("name each theme's kind and a light theme's dark twin", () => {
    const kinds = Object.fromEntries(spec.themes.map(({ id, kind, twin }) => [id, [kind, twin]]));
    expect(kinds).toEqual({
      midnight: ["dark", null],
      midday: ["light", "midnight"],
      twilight: ["dark", null],
      starlight: ["light", "moonlight"],
      moonlight: ["dark", null],
      nightshade: ["dark", null],
      greenlight: ["light", "nightshade"],
      sunshine: ["light", "midnight"],
      blacklight: ["dark", null],
    });
  });

  it("let an accent replace a theme's accent and logo, but Sunshine's logo and Blacklight's states", () => {
    expect(colorOf(spec, "midnight", null, "accent")).toBe("#38bdf8");
    expect(colorOf(spec, "midnight", "amber", "accent")).toBe(spec.accents.find(({ id }) => id === "amber")!.dark.accent);
    expect(colorOf(spec, "midday", "amber", "accent")).toBe(spec.accents.find(({ id }) => id === "amber")!.light.accent);
    // Amber moves warning aside, but not Blacklight's gray.
    expect(colorOf(spec, "moonlight", "amber", "warning")).not.toBe(colorOf(spec, "moonlight", null, "warning"));
    expect(colorOf(spec, "blacklight", "amber", "warning")).toBe(colorOf(spec, "blacklight", null, "warning"));
    expect(colorOf(spec, "blacklight", "amber", "brand")).not.toBe(colorOf(spec, "blacklight", null, "brand"));
    expect(colorOf(spec, "sunshine", "amber", "brand")).toBe(colorOf(spec, "sunshine", null, "brand"));
    // The tile is the theme's.
    expect(colorOf(spec, "twilight", "mint", "brand-tile")).toBe(colorOf(spec, "twilight", null, "brand-tile"));
  });
});
