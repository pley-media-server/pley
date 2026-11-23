import { describe, expect, it } from "vitest";
import type { Marker } from "../../src/api/types";
import { CreditsGate, reachesEnd } from "../../src/player/markers";
// Shared with the native Samsung app's tests, which must answer the same.
import fixture from "../fixtures/markers.json";

const marker = (kind: Marker["kind"], start_ms: number, end_ms: number, file_id = 7): Marker => ({ file_id, kind, start_ms, end_ms, source: "chapter" });

describe("credits that bring Up Next, as the shared fixture has them", () => {
  it("are those that run to the end of their file, within 15 s of it", () => {
    for (const c of fixture.up_next) {
      expect([c.name, reachesEnd(c.marker as Marker, c.duration_ms)]).toEqual([c.name, c.up_next]);
    }
  });
});

describe("the credits gate", () => {
  it("starts Up Next once for credits that run to the end, again for another file's or a new item", () => {
    const gate = new CreditsGate();
    const credits = marker("credits", 2_600_000, 2_700_000);
    expect(gate.enter(marker("intro", 10_000, 70_000), 2_700_000)).toBe(false);
    expect(gate.enter(credits, 2_700_000)).toBe(true);
    // Seeking back into the same credits.
    expect(gate.enter(credits, 2_700_000)).toBe(false);
    expect(gate.enter(marker("credits", 2_600_000, 2_700_000, 8), 2_700_000)).toBe(true);
    gate.reset();
    expect(gate.enter(credits, 2_700_000)).toBe(true);
  });

  it("never starts it for credits that something follows, nor in a file of unknown length", () => {
    const gate = new CreditsGate();
    expect(gate.enter(marker("credits", 2_520_000, 2_640_000), 2_700_000)).toBe(false);
    expect(gate.enter(marker("credits", 2_600_000, 2_700_000), null)).toBe(false);
    expect(gate.enter(marker("credits", 2_600_000, 2_700_000), 0)).toBe(false);
  });
});
