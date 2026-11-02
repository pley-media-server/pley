import { afterEach, describe, expect, it, vi } from "vitest";
import { androidPlayer, appShell } from "./shell";

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

describe("androidPlayer", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    delete (window as unknown as { PleyAndroid?: unknown }).PleyAndroid;
  });

  const request = { item_id: 7, start_ms: 90_000, file_id: null, audio: 2, sub: null, title: "Jaws" };

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
