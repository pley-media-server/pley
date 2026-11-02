import { describe, expect, it } from "vitest";
import { placeholderTitleSize } from "../../src/ui/kit/art";

describe("a placeholder's title", () => {
  it("keeps Title's 16 px while its longest word fits", () => {
    expect(placeholderTitleSize("12 Angry Men", 96)).toBe(16);
    expect(placeholderTitleSize("Singin' in the Rain", 96)).toBe(16);
    expect(placeholderTitleSize("", 96)).toBe(16);
  });

  it("gets smaller so that a long word fits whole", () => {
    // A poster's 72 px of room: "Casablanca" and "Chinatown" broke mid-word at 16 px.
    expect(placeholderTitleSize("Casablanca", 96)).toBe(12);
    expect(placeholderTitleSize("Chinatown", 96)).toBe(14);
    // A landscape tile has room for them.
    expect(placeholderTitleSize("Casablanca", 176)).toBe(16);
  });

  it("stops at 11 px", () => {
    expect(placeholderTitleSize("Supercalifragilisticexpialidocious", 96)).toBe(11);
  });
});
