import { describe, expect, it } from "vitest";
import type { Marker } from "../../src/api/types";
import { activeMarker, CreditsGate, reachesEnd, SKIP_SHOWN_MS, skipShows, SkipWindow } from "../../src/player/markers";
// Shared with the native Samsung app's tests, which must answer the same.
import fixture from "../fixtures/markers.json";
import shown from "../fixtures/skip-shown.json";

/** A step of skip-shown.json: where the playhead is at `ms` and whether the controls show, with what shows; or Up Next cancelled. */
type SkipStep = { ms: number; position_ms?: number; controls?: boolean; skip?: string | null; on_its_own?: boolean; up_next_cancelled?: boolean };

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

describe("when Skip shows, as the shared fixture has it", () => {
  it("is on its own for 8 s", () => {
    expect(SKIP_SHOWN_MS).toBe(shown.shown_ms);
  });

  for (const c of shown.cases) {
    it(c.name, () => {
      const rule = new SkipWindow();
      const markers = shown.markers as Marker[];
      for (const step of c.steps as SkipStep[]) {
        if (step.up_next_cancelled) {
          rule.again(step.ms);
          continue;
        }
        const m = activeMarker(markers, step.position_ms!);
        const alone = rule.at(m, step.ms);
        const what = skipShows(m !== null, alone, step.controls!) ? m!.kind : null;
        expect([step.ms, step.controls, what, alone]).toEqual([step.ms, step.controls, step.skip, step.on_its_own]);
      }
    });
  }

  it("starts afresh for a new item, even in the same marker", () => {
    const rule = new SkipWindow();
    const intro = marker("intro", 10_000, 70_000);
    expect(rule.at(intro, 0)).toBe(true);
    expect(rule.at(intro, 9_000)).toBe(false);
    rule.reset();
    expect(rule.at(intro, 10_000)).toBe(true);
  });
});
