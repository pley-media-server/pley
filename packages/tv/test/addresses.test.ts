import { describe, expect, it } from "vitest";
import { atHome, candidates, displayAddress, isLocalHost, normalizeServer, origin, unencrypted } from "../src/auth/addresses";
// Shared with the native Samsung app's tests, which must answer the same.
import fixture from "./fixtures/addresses.json";

describe("server addresses, as the shared fixture has them", () => {
  const rules = { normalizeServer, candidates, isLocalHost, atHome, unencrypted, displayAddress, origin };
  for (const [name, cases] of Object.entries(fixture)) {
    it(name, () => {
      const rule = rules[name as keyof typeof rules] as (input: string) => unknown;
      for (const c of cases) expect([c.in, rule(c.in)]).toEqual([c.in, c.out]);
    });
  }
});
