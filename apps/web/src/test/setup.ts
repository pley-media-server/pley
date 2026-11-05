import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach } from "vitest";
// The app's words, in English, as tests read them.
import "@/i18n";

// `findBy…`/`waitFor` wait up to 5 s: busy CI runners can be slow to render.
configure({ asyncUtilTimeout: 5000 });

/** Defines `name` on `target` when jsdom leaves it out (or undefined). */
function polyfill(target: object, name: string, value: unknown) {
  if (Reflect.get(target, name) == null) Object.defineProperty(target, name, { value, configurable: true, writable: true });
}

// What jsdom leaves out (layout, media queries, scrolling), as inert stand-ins.
class NoopResizeObserver implements ResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
polyfill(globalThis, "ResizeObserver", NoopResizeObserver);
polyfill(window, "matchMedia", (query: string): MediaQueryList => ({
  matches: false,
  media: query,
  onchange: null,
  addEventListener: () => {},
  removeEventListener: () => {},
  addListener: () => {},
  removeListener: () => {},
  dispatchEvent: () => false,
}));
for (const name of ["scrollIntoView", "scrollBy", "setPointerCapture", "releasePointerCapture"]) {
  polyfill(Element.prototype, name, () => {});
}
polyfill(Element.prototype, "hasPointerCapture", () => false);

afterEach(() => {
  cleanup();
  window.localStorage.clear();
  window.sessionStorage.clear();
});
