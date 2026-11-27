import { useEffect, useRef, useState } from "preact/hooks";
import { useUi } from "../context";
import { placeholderGradient } from "../images";

/** Where the dark comes up from the bottom: Bg from 0 % at `from` to 100 % at `to`, solid below. */
export const SCRIMS = {
  hero: { from: 220, to: 410 },
  rows: { from: 120, to: 300 },
  item: { from: 260, to: 500 },
} as const;

export type ScrimKind = keyof typeof SCRIMS;

/**
 * A full-screen picture behind a page (TvBackdrop): right of center and
 * above the middle, never blurred, under a scrim from the left and one
 * from the bottom. A new picture waits until it has loaded (at most 1 s),
 * then crossfades over 300 ms; at once on low-end TVs.
 */
export function Backdrop(p: { candidates: string[]; title: string; scrim: ScrimKind }) {
  const ui = useUi();
  const width = ui.tier === "low" ? 960 : 1280;
  const urls = p.candidates.map((c) => ui.images.url(c, width, width));
  const key = urls.join("|");
  // Two slots: the picture shown, and the one coming.
  const [slots, setSlots] = useState<{ a: string | null; b: string | null; top: "a" | "b" }>({ a: null, b: null, top: "a" });
  const release = useRef<(() => void)[]>([]);
  useEffect(() => {
    let cancelled = false;
    const releases: (() => void)[] = [];
    const tryAt = (i: number) => {
      const u = urls[i];
      if (!u || cancelled) return;
      if (ui.images.isMissing(u)) {
        tryAt(i + 1);
        return;
      }
      releases.push(
        ui.images.load(u, (objectUrl) => {
          if (cancelled) return;
          if (!objectUrl) {
            tryAt(i + 1);
            return;
          }
          whenDecoded(objectUrl, () => {
            if (cancelled) return;
            setSlots((s) => {
              const shown = s.top === "a" ? s.a : s.b;
              if (shown === objectUrl) return s;
              return s.top === "a" ? { a: s.a, b: objectUrl, top: "b" } : { a: objectUrl, b: s.b, top: "a" };
            });
          });
        }, -1),
      );
    };
    tryAt(0);
    const before = release.current;
    release.current = releases;
    // The picture before stays until the new one has faded in.
    const t = setTimeout(() => before.forEach((r) => r()), 400);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [key]);
  useEffect(() => () => release.current.forEach((r) => r()), []);
  const scrim = SCRIMS[p.scrim];
  const instant = ui.tier === "low" || ui.reducedMotion;
  const fade = instant ? "transition:none;" : "";
  return (
    <div class="backdrop" aria-hidden="true">
      <div class="ph" style={{ background: placeholderGradient(p.title) }} />
      {slots.a && <img src={slots.a} alt="" style={`${fade}opacity:${slots.top === "a" ? 1 : 0}`} />}
      {slots.b && <img src={slots.b} alt="" style={`${fade}opacity:${slots.top === "b" ? 1 : 0}`} />}
      <div class="left-scrim" />
      <div
        class="bottom-scrim"
        style={`top:${scrim.from}px;background:linear-gradient(to bottom, rgba(var(--bg-rgb), 0) 0, rgba(var(--bg-rgb), 1) ${scrim.to - scrim.from}px, rgba(var(--bg-rgb), 1) 100%)`}
      />
    </div>
  );
}

/**
 * Calls `then` once the picture at `url` is decoded (img.decode, Chromium
 * 64), else once it has loaded: a crossfade that starts then never waits
 * on a 1280-wide picture's decoding half way through.
 */
function whenDecoded(url: string, then: () => void): void {
  const img = new Image();
  img.src = url;
  if (typeof img.decode === "function") {
    img.decode().then(then, then);
  } else if (img.complete) {
    then();
  } else {
    img.onload = () => then();
    img.onerror = () => then();
  }
}

/**
 * The value last seen at rest (the Android app's rememberRested): the
 * first one at once, later ones once focus has rested 400 ms with no key
 * held.
 */
export function useRested<T>(value: T): T {
  const ui = useUi();
  const [rested, setRested] = useState(value);
  useEffect(() => {
    if (value === rested) return;
    let t: ReturnType<typeof setTimeout>;
    const check = () => {
      const wait = ui.input.gate.untilRest();
      if (wait <= 0) setRested(value);
      else t = setTimeout(check, Math.min(wait, 100));
    };
    check();
    return () => clearTimeout(t);
  }, [value]);
  return rested;
}
