import "./polyfills";
import "./ui/theme.css";
import "./ui/screens.css";
import { render } from "preact";
import { effect } from "@preact/signals";
import type { Platform } from "./platform";
import { AppState } from "./state";
import { App } from "./ui/shell/app";
import { applyTheme } from "./ui/themes";

export type { Platform, DeviceFacts, FoundServer } from "./platform";
export { AppState } from "./state";

/**
 * Draws the app into `#app`: the 960×540 canvas zoomed to the screen
 * (2× on the TVs' 1920×1080 web layer), so text and pictures are drawn
 * at the screen's own pixels.
 */
export function startApp(platform: Platform): AppState {
  const root = document.getElementById("app")!;
  const style = root.style as CSSStyleDeclaration & { zoom?: string };
  const fit = () => {
    const s = Math.max(0.25, Math.min(window.innerWidth / 960, window.innerHeight / 540) || 2);
    if ("zoom" in document.body.style) style.zoom = String(s);
    else style.transform = `scale(${s})`;
    return s;
  };
  const scale = fit();
  // TVs never resize; a desktop browser (development) does.
  window.addEventListener("resize", fit);
  const state = new AppState(platform);
  // The theme picked on this TV, before the first frame; a pick redraws every color in the next one.
  effect(() => applyTheme(state.theme.value, state.accent.value));
  render(<App state={state} scale={scale} />, root);
  return state;
}
