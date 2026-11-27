import { afterEach, describe, expect, it, vi } from "vitest";
import themeScript from "../../public/theme.js?raw";
import { logoIcon } from "@/components/Logo";
import { renderHook } from "@testing-library/react";
import { ACCENT_KEY, ACCENTS, currentAccent, currentTheme, isAccent, isTheme, onTheme, showAccent, showProfile, showTheme, startTheme, THEME_KEY, THEMES, useOutsideProfile } from ".";

afterEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.accent;
});

describe("themes", () => {
  it("are Pley's nine, Midnight first", () => {
    expect(THEMES).toEqual(["midnight", "midday", "twilight", "starlight", "moonlight", "nightshade", "greenlight", "sunshine", "blacklight"]);
    expect(isTheme("twilight")).toBe(true);
    for (const wrong of ["light", "Midnight", "", null, 3]) expect(isTheme(wrong)).toBe(false);
  });

  it("start in this device's last, else Midnight", () => {
    startTheme();
    expect(currentTheme()).toBe("midnight");
    expect(document.documentElement.dataset.theme).toBe("midnight");

    localStorage.setItem(THEME_KEY, "moonlight");
    startTheme();
    expect(currentTheme()).toBe("moonlight");
    expect(document.documentElement.dataset.theme).toBe("moonlight");

    // One a later Pley kept, which this one doesn't draw.
    localStorage.setItem(THEME_KEY, "dusk");
    startTheme();
    expect(currentTheme()).toBe("midnight");
  });

  it("show on the page, are kept for the next visit, and tell who follows", () => {
    startTheme();
    const listener = vi.fn();
    const stop = onTheme(listener);
    showTheme("starlight");
    expect(document.documentElement.dataset.theme).toBe("starlight");
    expect(localStorage.getItem(THEME_KEY)).toBe("starlight");
    expect(listener).toHaveBeenCalledTimes(1);
    showTheme("starlight");
    expect(listener).toHaveBeenCalledTimes(1);
    stop();
  });

  it("follow a pick made in another tab", () => {
    startTheme();
    const listener = vi.fn();
    const stop = onTheme(listener);
    // Another tab writes the storage; this one hears of it.
    localStorage.setItem(THEME_KEY, "starlight");
    localStorage.setItem(ACCENT_KEY, "amber");
    window.dispatchEvent(new StorageEvent("storage", { key: ACCENT_KEY }));
    expect(currentTheme()).toBe("starlight");
    expect(currentAccent()).toBe("amber");
    expect(document.documentElement.dataset.theme).toBe("starlight");
    expect(document.documentElement.dataset.accent).toBe("amber");
    expect(listener).toHaveBeenCalledTimes(1);

    // Another key, or nothing new, changes nothing.
    window.dispatchEvent(new StorageEvent("storage", { key: "pley.language" }));
    window.dispatchEvent(new StorageEvent("storage", { key: THEME_KEY }));
    expect(listener).toHaveBeenCalledTimes(1);
    stop();
  });

  it("color the tab's icon as the logo", () => {
    const colors: Record<string, string> = {
      "--color-brand-light": "#ffd3a6",
      "--color-brand": "#ff79b0",
      "--color-brand-deep": "#c2448f",
      "--color-brand-far": "#6e2c7a",
      "--color-brand-tile": "#1f1233",
    };
    const style = { getPropertyValue: (name: string) => ` ${colors[name] ?? ""}` } as CSSStyleDeclaration;
    const icon = new DOMParser().parseFromString(logoIcon(style), "image/svg+xml");
    expect(icon.querySelector("parsererror")).toBeNull();
    expect(icon.querySelector("rect")?.getAttribute("fill")).toBe("#1f1233");
    expect([...icon.querySelectorAll("stop")].map((stop) => stop.getAttribute("stop-color"))).toEqual(["#ffd3a6", "#ff79b0"]);
    expect([...icon.querySelectorAll("path")].map((path) => path.getAttribute("fill"))).toEqual(["#6e2c7a", "#c2448f", "url(#front)"]);
  });

  it("are put on the page before it draws, from where the app keeps them", () => {
    // public/theme.js runs before the app's code, which keeps the theme where it reads.
    localStorage.setItem(THEME_KEY, "twilight");
    expect(themeScript).toContain(JSON.stringify(THEME_KEY));
    // Run as the page runs it.
    // eslint-disable-next-line @typescript-eslint/no-implied-eval
    const run = new Function(themeScript) as () => void;
    run();
    expect(document.documentElement.dataset.theme).toBe("twilight");
  });
});

describe("each profile", () => {
  it("keeps its own theme and accent on this device, a new one starting in Midnight", () => {
    startTheme();
    showProfile(1);
    showTheme("starlight");
    showAccent("amber");
    expect(localStorage.getItem(`${THEME_KEY}.1`)).toBe("starlight");
    expect(localStorage.getItem(`${ACCENT_KEY}.1`)).toBe("amber");

    // Another profile, new to this device, starts in Midnight in its own accent.
    showProfile(2);
    expect(currentTheme()).toBe("midnight");
    expect(currentAccent()).toBeNull();
    expect(document.documentElement.dataset.theme).toBe("midnight");
    showTheme("sunshine");

    // Each comes back to its own.
    showProfile(1);
    expect(currentTheme()).toBe("starlight");
    expect(currentAccent()).toBe("amber");
    showProfile(2);
    expect(currentTheme()).toBe("sunshine");
    expect(currentAccent()).toBeNull();
  });

  it("leave the one shown last for the next visit, before a profile is known", () => {
    startTheme();
    showProfile(1);
    showTheme("twilight");
    showAccent("mint");
    showProfile(2);
    // The next visit starts as profile 2 left it, until it knows the profile.
    startTheme();
    expect(currentTheme()).toBe("midnight");
    expect(currentAccent()).toBeNull();
    showProfile(1);
    startTheme();
    expect(currentTheme()).toBe("twilight");
    expect(currentAccent()).toBe("mint");
  });

  it("follows a pick made for it in another tab, and not one made for another profile", () => {
    startTheme();
    showProfile(1);
    const listener = vi.fn();
    const stop = onTheme(listener);
    localStorage.setItem(`${THEME_KEY}.2`, "greenlight");
    window.dispatchEvent(new StorageEvent("storage", { key: `${THEME_KEY}.2` }));
    expect(currentTheme()).toBe("midnight");
    localStorage.setItem(`${THEME_KEY}.1`, "moonlight");
    window.dispatchEvent(new StorageEvent("storage", { key: `${THEME_KEY}.1` }));
    expect(currentTheme()).toBe("moonlight");
    expect(listener).toHaveBeenCalledTimes(1);
    stop();
  });
});

describe("outside a profile", () => {
  it("is Midnight in its own accent, the profile's own coming back after", () => {
    startTheme();
    showProfile(1);
    showTheme("starlight");
    showAccent("amber");
    // Signing in, "Who's watching?", setup: no profile's theme.
    const screen = renderHook(() => useOutsideProfile());
    expect(currentTheme()).toBe("midnight");
    expect(currentAccent()).toBeNull();
    expect(document.documentElement.dataset.theme).toBe("midnight");
    // A reload there starts in Midnight too.
    expect(localStorage.getItem(THEME_KEY)).toBe("midnight");
    expect(localStorage.getItem(ACCENT_KEY)).toBeNull();
    // The profile keeps its pick.
    expect(localStorage.getItem(`${THEME_KEY}.1`)).toBe("starlight");
    screen.unmount();
    expect(currentTheme()).toBe("starlight");
    expect(currentAccent()).toBe("amber");
    expect(localStorage.getItem(THEME_KEY)).toBe("starlight");
  });

  it("stays Midnight when the profile becomes known under it", () => {
    startTheme();
    localStorage.setItem(`${THEME_KEY}.3`, "twilight");
    const screen = renderHook(() => useOutsideProfile());
    showProfile(3);
    expect(currentTheme()).toBe("midnight");
    screen.unmount();
    expect(currentTheme()).toBe("twilight");
  });
});

describe("accents", () => {
  it("are Pley's six, none being the theme's own", () => {
    expect(ACCENTS).toEqual(["sky", "violet", "miami", "gold", "mint", "amber"]);
    expect(isAccent("gold")).toBe(true);
    for (const wrong of ["own", "Gold", "", null, 3]) expect(isAccent(wrong)).toBe(false);
  });

  it("start in this device's last, else the theme's own", () => {
    startTheme();
    expect(currentAccent()).toBeNull();
    expect(document.documentElement.dataset.accent).toBeUndefined();

    localStorage.setItem(ACCENT_KEY, "violet");
    startTheme();
    expect(currentAccent()).toBe("violet");
    expect(document.documentElement.dataset.accent).toBe("violet");

    // One a later Pley kept, which this one doesn't draw.
    localStorage.setItem(ACCENT_KEY, "coral");
    startTheme();
    expect(currentAccent()).toBeNull();
    expect(document.documentElement.dataset.accent).toBeUndefined();
  });

  it("show on the page over the theme, are kept for the next visit, and tell who follows", () => {
    startTheme();
    const listener = vi.fn();
    const stop = onTheme(listener);
    showAccent("mint");
    expect(document.documentElement.dataset.accent).toBe("mint");
    expect(localStorage.getItem(ACCENT_KEY)).toBe("mint");
    expect(listener).toHaveBeenCalledTimes(1);
    showAccent("mint");
    expect(listener).toHaveBeenCalledTimes(1);

    // A theme picked after keeps the accent.
    showTheme("sunshine");
    expect(document.documentElement.dataset.accent).toBe("mint");

    showAccent(null);
    expect(document.documentElement.dataset.accent).toBeUndefined();
    expect(localStorage.getItem(ACCENT_KEY)).toBeNull();
    expect(currentTheme()).toBe("sunshine");
    expect(listener).toHaveBeenCalledTimes(3);
    stop();
  });

  it("are told to the desktop app with the theme, as they show", () => {
    const theme = vi.fn();
    const desktop = window as unknown as { __PLEY_APP__?: string; __PLEY_DESKTOP__?: unknown };
    desktop.__PLEY_APP__ = "desktop";
    desktop.__PLEY_DESKTOP__ = { theme };
    try {
      localStorage.setItem(THEME_KEY, "moonlight");
      startTheme();
      expect(theme).toHaveBeenLastCalledWith("moonlight", null);
      showAccent("violet");
      expect(theme).toHaveBeenLastCalledWith("moonlight", "violet");
      showTheme("sunshine");
      expect(theme).toHaveBeenLastCalledWith("sunshine", "violet");
      showAccent(null);
      expect(theme).toHaveBeenLastCalledWith("sunshine", null);
    } finally {
      delete desktop.__PLEY_APP__;
      delete desktop.__PLEY_DESKTOP__;
    }
  });

  it("are put on the page before it draws, from where the app keeps them", () => {
    localStorage.setItem(ACCENT_KEY, "miami");
    expect(themeScript).toContain(JSON.stringify(ACCENT_KEY));
    // eslint-disable-next-line @typescript-eslint/no-implied-eval
    const run = new Function(themeScript) as () => void;
    run();
    expect(document.documentElement.dataset.accent).toBe("miami");
  });
});
