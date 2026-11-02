import { describe, expect, it } from "vitest";
import type { HouseholdProfile } from "../src/api/types";
import { TokenError } from "../src/auth/tokens";
import { profileChoice, profileTiles, refusal } from "../src/ui/screens/onboarding/profiles";
// Shared with the native Samsung app's tests, which must answer the same.
import fixture from "./fixtures/profiles.json";

const household = (list: unknown) => list as HouseholdProfile[];

describe("Who's watching?, as the shared fixture has it", () => {
  it("tiles", () => {
    for (const c of fixture.tiles) {
      const tiles = profileTiles(household(c.in.household), c.in.me).map(({ id, current, needs }) => ({ id, current, needs }));
      expect([c.in, tiles]).toEqual([c.in, c.out]);
    }
  });

  it("choice", () => {
    for (const c of fixture.choice) {
      const [picked] = profileTiles(household(c.in.household), c.in.me).filter((p) => p.id === c.in.picked);
      if (!picked) throw new Error(`no profile ${c.in.picked} in the household`);
      expect([c.in, profileChoice(picked, c.in.me, c.in.gate)]).toEqual([c.in, c.out]);
    }
  });

  it("refusal", () => {
    for (const c of fixture.refusal) {
      const r = refusal(new TokenError(400, c.in.oauth), c.in.where !== "pick", c.in.where === "confirm");
      expect([c.in, r]).toEqual([c.in, c.out]);
    }
  });
});
