import { describe, expect, it } from "vitest";

/** The player's sources, which draw over the video, as text. */
const sources = import.meta.glob<string>(["./*.tsx", "./*.ts", "!./*.test.ts", "!./*.test.tsx"], {
  query: "?raw",
  import: "default",
  eager: true,
});

describe("what the player draws over the video", () => {
  it("has no backdrop filter", () => {
    // Chrome takes a video a backdrop filter covers off its overlay plane
    // and draws it itself: its colors change as a menu or a tooltip opens.
    expect(Object.keys(sources).length).toBeGreaterThan(10);
    const filtered = Object.entries(sources)
      .filter(([, text]) => /\bbackdrop-(?:blur|brightness|contrast|grayscale|hue-rotate|invert|opacity|saturate|sepia)|backdrop-filter/.test(text))
      .map(([file]) => file);
    expect(filtered).toEqual([]);
  });
});
