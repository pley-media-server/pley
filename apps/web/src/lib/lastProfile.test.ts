import { describe, expect, it } from "vitest";
import { testUser } from "@/test/render";
import { asksWhoIsWatching, lastProfile, rememberProfile } from "./lastProfile";

describe("lastProfile", () => {
  it("is the profile this browser was used as last", () => {
    expect(lastProfile()).toBeNull();
    rememberProfile(3);
    expect(lastProfile()).toBe(3);
    rememberProfile(5);
    expect(lastProfile()).toBe(5);
  });

  it("ignores what isn't a profile's id", () => {
    window.localStorage.setItem("pley.lastProfile", '"3"');
    expect(lastProfile()).toBeNull();
    window.localStorage.setItem("pley.lastProfile", "-1");
    expect(lastProfile()).toBeNull();
  });
});

describe("asksWhoIsWatching", () => {
  const anna = { ...testUser, id: 1, has_pin: false };

  it("goes straight in as the profile used last when it has no PIN", () => {
    expect(asksWhoIsWatching(anna, 3, 1)).toBe(false);
  });

  it("asks when that profile has a PIN, another was used last, or none was", () => {
    expect(asksWhoIsWatching({ ...anna, has_pin: true }, 3, 1)).toBe(true);
    expect(asksWhoIsWatching(anna, 3, 2)).toBe(true);
    expect(asksWhoIsWatching(anna, 3, null)).toBe(true);
  });

  it("never asks with a single profile", () => {
    expect(asksWhoIsWatching({ ...anna, has_pin: true }, 1, null)).toBe(false);
    expect(asksWhoIsWatching(anna, 0, null)).toBe(false);
  });
});
