import type { CSSProperties } from "preact";
import { useEffect, useState } from "preact/hooks";
import { markLayers, type MarkGlyph } from "pley-placeholder";
import type { ItemSummary } from "../../api/types";
import { useUi } from "../context";
import { placeholderGradient } from "../images";

/** Which picture of an item a place shows. */
export type ArtKind = "poster" | "thumb" | "backdrop";

/**
 * The addresses to try for an item's picture, best first (the Android
 * app's ArtworkImage): a poster falls back to the season's and the show's;
 * a still to the backdrop, an episode's to its show's.
 */
export function artCandidates(item: ItemSummary, kind: ArtKind): string[] {
  const out: string[] = [];
  const add = (u: string | null | undefined) => {
    if (u && out.indexOf(u) < 0) out.push(u);
  };
  if (kind === "poster") {
    add(item.poster_url);
    if (item.parent_id) add(`/api/items/${item.parent_id}/images/poster`);
    if (item.grandparent_id) add(`/api/items/${item.grandparent_id}/images/poster`);
  } else if (kind === "thumb") {
    if (item.kind === "movie") add(item.backdrop_url);
    add(item.thumb_url);
    if (item.kind === "episode" && item.grandparent_id) add(`/api/items/${item.grandparent_id}/images/backdrop`);
    if (item.kind !== "episode") add(item.backdrop_url);
  } else {
    add(item.backdrop_url);
    if (item.grandparent_id) add(`/api/items/${item.grandparent_id}/images/backdrop`);
    if (item.parent_id) add(`/api/items/${item.parent_id}/images/backdrop`);
  }
  return out;
}

/**
 * Pley's mark (packages/placeholder): its three posters back to front,
 * each lighter than the one behind, `glyph` cut out of the front one.
 */
function Mark(p: { glyph: MarkGlyph }) {
  return (
    <svg class="tile-mark" viewBox="0 0 64 64" aria-hidden="true">
      {markLayers(p.glyph).map((layer) => (
        <path key={layer.alpha} fill="#fff" fill-opacity={layer.alpha} fill-rule={layer.evenOdd ? "evenodd" : undefined} d={layer.path} />
      ))}
    </svg>
  );
}

/**
 * One picture from its candidates, `width` canvas px wide: the first that
 * loads wins. Until then the title's gradient, with Pley's mark on it once
 * none will come (no candidates, or none loaded) when given `mark`; not
 * while one loads, so a row scrolling in doesn't flash marks. Only a
 * picture whose name is beside or under it gets a mark (a tile's caption, a
 * row's line), so the mark never stands for a title no one can read.
 * Fades in over 150 ms unless it was in memory already.
 */
export function Art(props: {
  candidates: string[];
  width: number;
  /** What the placeholder's gradient is made from. */
  title: string;
  /** Pley's mark with this glyph (`markGlyph(item.kind)`) once no picture will come. */
  mark?: MarkGlyph;
  class?: string;
  style?: CSSProperties;
  /** The widest picture asked for, in px (backdrops: 1280). */
  cap?: number;
  priority?: number;
}) {
  const ui = useUi();
  const px = props.width * ui.scale;
  const urls = props.candidates.map((c) => ui.images.url(c, px, props.cap));
  const key = urls.join("|");
  const firstReady = urls.map((u) => ui.images.ready(u)).filter(Boolean)[0] ?? null;
  // "none": every candidate was tried and none could be had.
  const [src, setSrc] = useState<{ key: string; url: string | null; instant: boolean; none: boolean }>({
    key,
    url: firstReady,
    instant: !!firstReady,
    none: urls.length === 0,
  });
  useEffect(() => {
    let release = () => {};
    let cancelled = false;
    const tryAt = (i: number) => {
      if (cancelled) return;
      const u = urls[i];
      if (!u) {
        setSrc({ key, url: null, instant: true, none: true });
        return;
      }
      if (ui.images.isMissing(u)) {
        tryAt(i + 1);
        return;
      }
      const ready = ui.images.ready(u);
      release = ui.images.load(
        u,
        (objectUrl) => {
          if (cancelled) return;
          if (objectUrl) setSrc({ key, url: objectUrl, instant: !!ready || ui.tier === "low", none: false });
          else {
            release();
            tryAt(i + 1);
          }
        },
        props.priority ?? 0,
      );
    };
    tryAt(0);
    return () => {
      cancelled = true;
      release();
    };
  }, [key]);
  const shown = src.key === key ? src.url : firstReady;
  const instant = src.key === key ? src.instant : !!firstReady;
  const none = src.key === key ? src.none : urls.length === 0;
  // "loaded": the picture is in; "covered": its fade is over, so the placeholder can go.
  const [state, setState] = useState<{ url: string | null; loaded: boolean; covered: boolean }>({ url: null, loaded: false, covered: false });
  const loaded = state.url === shown && state.loaded;
  const covered = state.url === shown && state.covered;
  useEffect(() => {
    if (!loaded || covered) return;
    const t = setTimeout(() => setState((s) => (s.url === shown ? { ...s, covered: true } : s)), instant ? 0 : 160);
    return () => clearTimeout(t);
  }, [loaded, covered, shown]);
  return (
    <div class={props.class} style={props.style}>
      {!covered && (
        <div class="tile-placeholder" style={{ background: placeholderGradient(props.title) }}>
          {props.mark && none && !shown && <Mark glyph={props.mark} />}
        </div>
      )}
      {shown && (
        <img
          class={`tile-img${loaded ? " in" : ""}${instant ? " instant" : ""}`}
          src={shown}
          alt=""
          decoding="async"
          onLoad={(e) => {
            // Its fade starts once it is decoded, not while it still is.
            const done = () => setState({ url: shown, loaded: true, covered: false });
            const img = e.currentTarget as HTMLImageElement;
            if (typeof img.decode === "function") img.decode().then(done, done);
            else done();
          }}
        />
      )}
    </div>
  );
}
