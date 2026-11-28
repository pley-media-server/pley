/**
 * The pley apps (desktop, Android) show this web app from the server and
 * tell it so: the desktop app with `window.__PLEY_APP__`, the Android app in
 * the user agent (`pleyAndroid/0.1.0`).
 */
export type Shell = "desktop" | "android";

export function appShell(userAgent: string = typeof navigator === "undefined" ? "" : navigator.userAgent): Shell | null {
  // The desktop app marks every page (apps/desktop/src-tauri/src/lib.rs).
  const marked = typeof window === "undefined" ? undefined : (window as { __PLEY_APP__?: unknown }).__PLEY_APP__;
  if (marked === "desktop" || marked === "android") return marked;
  if (/\bpleyDesktop\//.test(userAgent)) return "desktop";
  if (/\bpleyAndroid\//.test(userAgent)) return "android";
  return null;
}

/**
 * Tells the desktop app the theme and accent this page shows (null for the
 * theme's own), for the pages it draws itself, which can't read this
 * page's storage. The app gives its window `__PLEY_DESKTOP__` for this
 * (apps/desktop/src-tauri/src/lib.rs); older versions don't, and other
 * apps have their own themes.
 */
export function tellShellTheme(theme: string, accent: string | null): void {
  if (typeof window === "undefined" || appShell() !== "desktop") return;
  const bridge = (window as { __PLEY_DESKTOP__?: { theme?: (theme: string, accent: string | null) => void } }).__PLEY_DESKTOP__;
  if (typeof bridge?.theme === "function") bridge.theme(theme, accent);
}

/** The event the Android app sends when the web app shows again (after its player, say). */
export const APP_RESUMED = "pley:resume";

/** A path the apps catch on their own: back to their list of servers. */
export const SWITCH_SERVER_PATH = "/__app/servers";

/** What the Android app's player needs to start a video. */
export interface NativePlayRequest {
  item_id: number;
  start_ms: number;
  file_id: number | null;
  audio: number | null;
  sub: number | null;
  title: string;
}

/**
 * The Android app's bridge to its own player, which plays more files as
 * they are. The app offers it to the server's own pages only.
 */
interface AndroidBridge {
  /** Hands the request to the native player (a message listener). */
  postMessage?: (request: string) => void;
  /** The same in older versions of the app; `false` when it can't play this. */
  play?: (request: string) => boolean;
}

export function androidPlayer(): ((request: NativePlayRequest) => boolean) | null {
  if (typeof window === "undefined" || appShell() !== "android") return null;
  const bridge = (window as unknown as { PleyAndroid?: AndroidBridge }).PleyAndroid;
  const { postMessage, play } = bridge ?? {};
  if (typeof postMessage === "function") {
    return (request) => {
      postMessage.call(bridge, JSON.stringify(request));
      return true;
    };
  }
  if (typeof play === "function") return (request) => play.call(bridge, JSON.stringify(request)) === true;
  return null;
}
