import { describe, expect, it } from "vitest";
import { addLabel, inBoth } from "./labels";

describe("addLabel", () => {
  it("adds a trimmed label once, whatever its case", () => {
    expect(addLabel([], "  Kids OK ")).toEqual(["Kids OK"]);
    expect(addLabel(["Kids OK"], "kids ok")).toEqual(["Kids OK"]);
    expect(addLabel(["Kids OK"], "  ")).toEqual(["Kids OK"]);
    expect(addLabel([], "x".repeat(130))).toEqual(["x".repeat(120)]);
  });
});

describe("inBoth", () => {
  it("finds labels always and never shown", () => {
    expect(inBoth(["Kids OK", "Scary"], ["scary"])).toEqual(["Scary"]);
    expect(inBoth(["Kids OK"], ["Scary"])).toEqual([]);
  });
});
