import { afterEach, describe, expect, it, vi } from "vitest";
import { androidPlayer, appShell, tellShellTheme } from "./shell";

const CHROME = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";

describe("appShell", () => {
  it("knows the apps by their user agent", () => {
    expect(appShell(`${CHROME} pleyDesktop/0.1.0`)).toBe("desktop");
    expect(appShell("Mozilla/5.0 (Linux; Android 15; wv) Chrome/140.0 Mobile Safari/537.36 pleyAndroid/0.1.0")).toBe("android");
    expect(appShell(CHROME)).toBeNull();
    expect(appShell("notpleyDesktop/1")).toBeNull();
  });

  it("knows the desktop app by the global it sets", () => {
    (window as unknown as { __PLEY_APP__?: string }).__PLEY_APP__ = "desktop";
    try {
      expect(appShell(CHROME)).toBe("desktop");
    } finally {
      delete (window as unknown as { __PLEY_APP__?: string }).__PLEY_APP__;
    }
    expect(appShell(CHROME)).toBeNull();
  });
});

describe("tellShellTheme", () => {
  const desktop = window as unknown as { __PLEY_APP__?: string; __PLEY_DESKTOP__?: unknown };

  afterEach(() => {
    vi.unstubAllGlobals();
    delete desktop.__PLEY_APP__;
    delete desktop.__PLEY_DESKTOP__;
  });

  it("tells the desktop app the theme and accent shown", () => {
    const theme = vi.fn();
    desktop.__PLEY_APP__ = "desktop";
    desktop.__PLEY_DESKTOP__ = { theme };
    tellShellTheme("sunshine", "violet");
    tellShellTheme("midnight", null);
    expect(theme.mock.calls).toEqual([
      ["sunshine", "violet"],
      ["midnight", null],
    ]);
  });

  it("tells no one in a browser, in older desktop apps or in the Android app", () => {
    const theme = vi.fn();
    desktop.__PLEY_DESKTOP__ = { theme };
    tellShellTheme("sunshine", null);
    vi.stubGlobal("navigator", { userAgent: `${CHROME} pleyAndroid/0.1.0` });
    tellShellTheme("sunshine", null);
    expect(theme).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
    desktop.__PLEY_APP__ = "desktop";
    delete desktop.__PLEY_DESKTOP__;
    expect(() => tellShellTheme("sunshine", null)).not.toThrow();
  });
});

describe("androidPlayer", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    delete (window as unknown as { PleyAndroid?: unknown }).PleyAndroid;
  });

  const request = { item_id: 7, start_ms: 90_000, file_id: null, audio: 2, sub: null, title: "Heat" };

  it("hands the request to the Android app as JSON", () => {
    vi.stubGlobal("navigator", { userAgent: `${CHROME} pleyAndroid/0.1.0` });
    const postMessage = vi.fn<(request: string) => void>();
    (window as unknown as { PleyAndroid: unknown }).PleyAndroid = { postMessage };
    expect(androidPlayer()?.(request)).toBe(true);
    expect(JSON.parse(postMessage.mock.calls[0]?.[0] ?? "null")).toEqual(request);
  });

  it("still talks to older versions of the app", () => {
    vi.stubGlobal("navigator", { userAgent: `${CHROME} pleyAndroid/0.1.0` });
    const play = vi.fn<(request: string) => boolean>(() => true);
    (window as unknown as { PleyAndroid: unknown }).PleyAndroid = { play };
    expect(androidPlayer()?.(request)).toBe(true);
    expect(JSON.parse(play.mock.calls[0]?.[0] ?? "null")).toEqual(request);
  });

  it("is missing in browsers, and when the bridge is", () => {
    expect(androidPlayer()).toBeNull();
    vi.stubGlobal("navigator", { userAgent: `${CHROME} pleyAndroid/0.1.0` });
    expect(androidPlayer()).toBeNull();
  });
});
