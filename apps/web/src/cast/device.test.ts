import { afterEach, describe, expect, it } from "vitest";
import { castSelf, DESKTOP_NAME, eventsUrl } from "./device";

type Marked = { __PLEY_APP__?: string; __PLEY_DEVICE_NAME__?: unknown };
const marked = window as unknown as Marked;

afterEach(() => {
  delete marked.__PLEY_APP__;
  delete marked.__PLEY_DEVICE_NAME__;
});

describe("castSelf", () => {
  it("is nobody in a browser: every tab would play", () => {
    expect(castSelf()).toBeNull();
    expect(eventsUrl(castSelf())).toBe("/api/events");
  });

  it("is the desktop app, under an id it makes once and keeps", () => {
    marked.__PLEY_APP__ = "desktop";
    const self = castSelf();
    expect(self?.id).toMatch(/^[0-9a-f]{32}$/);
    expect(self?.platform).toBe("desktop");
    expect(castSelf()?.id).toBe(self?.id);
    expect(window.localStorage.getItem("pley.castDevice")).toBe(self?.id);
  });

  it("keeps an id made before", () => {
    marked.__PLEY_APP__ = "desktop";
    window.localStorage.setItem("pley.castDevice", "0123456789abcdef0123456789abcdef");
    expect(castSelf()?.id).toBe("0123456789abcdef0123456789abcdef");
  });

  it("is named after the computer, as the app tells it, or else after the app", () => {
    marked.__PLEY_APP__ = "desktop";
    expect(castSelf()?.name).toBe(DESKTOP_NAME);
    marked.__PLEY_DEVICE_NAME__ = "  Study\u0007 PC ";
    expect(castSelf()?.name).toBe("Study PC");
    marked.__PLEY_DEVICE_NAME__ = "x".repeat(80);
    expect(castSelf()?.name).toHaveLength(64);
    marked.__PLEY_DEVICE_NAME__ = " ";
    expect(castSelf()?.name).toBe(DESKTOP_NAME);
  });

  it("says so as it opens the event stream", () => {
    marked.__PLEY_APP__ = "desktop";
    marked.__PLEY_DEVICE_NAME__ = "Study PC";
    const self = castSelf();
    const url = new URL(eventsUrl(self), "http://pley.test");
    expect(url.pathname).toBe("/api/events");
    expect(Object.fromEntries(url.searchParams)).toEqual({ cast_device: self?.id, cast_name: "Study PC", cast_platform: "desktop" });
  });
});
