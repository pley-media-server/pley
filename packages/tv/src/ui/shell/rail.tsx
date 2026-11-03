import { useSignal } from "@preact/signals";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import type { Library, RequestSummary, User } from "../../api/types";
import { hasRequests, mayAsk, requestsCount, requestsTitle } from "../../requests";
import { useUi } from "../context";
import { F, G } from "../kit/focusable";
import { Icon, type IconName } from "../kit/icons";
import type { PageKey } from "./navigator";

export interface RailEntry {
  id: string;
  label: string;
  icon: IconName;
  key: PageKey;
  /** A number beside the icon (what waits in requests), in the danger colour with `danger`. */
  badge?: { count: number; danger: boolean };
}

/** At most this many libraries in the rail; with more, Libraries lists them all. */
const MAX_LIBRARIES = 6;

/**
 * The rail's geometry on the 960×540 canvas (theme.css): entries 40 tall
 * and 4 apart, from y 67 under the avatar to Settings at y 473, so nine
 * show at once; the rest scroll.
 */
export const RAIL_ENTRY = 40;
export const RAIL_GAP = 4;
const RAIL_PITCH = RAIL_ENTRY + RAIL_GAP;
export const RAIL_FIRST = 67;
export const RAIL_SETTINGS = 473;
/** The entries' view: from the first entry's place to the gap above Settings. */
const RAIL_VIEW = RAIL_SETTINGS - RAIL_GAP - RAIL_FIRST;

/**
 * How far the rail's entries are scrolled once entry `focused` of `count`
 * has focus, from `scroll`: the entry wholly in view, and, moving `dir`
 * ("down" from above it or the avatar, "up" from below or Settings), the
 * next one that way too, so what is beyond the edge shows coming; within
 * the first entry at its place and the last ending above Settings. With no
 * direction (the rail opening on its current entry, the entries made
 * again) only the entry is brought into view. With no entry focused, the
 * scroll is kept within its ends.
 */
export function railScroll(scroll: number, focused: number, count: number, dir: "up" | "down" | null): number {
  const max = Math.max(0, count * RAIL_PITCH - RAIL_GAP - RAIL_VIEW);
  let s = scroll;
  if (focused >= 0 && focused < count) {
    const top = focused * RAIL_PITCH - (dir === "up" && focused > 0 ? RAIL_PITCH : 0);
    const bottom = focused * RAIL_PITCH + RAIL_ENTRY + (dir === "down" && focused + 1 < count ? RAIL_PITCH : 0);
    if (bottom - s > RAIL_VIEW) s = bottom - RAIL_VIEW;
    if (top < s) s = top;
  }
  return Math.max(0, Math.min(max, s));
}

export interface RailEntries {
  top: RailEntry[];
  settings: RailEntry;
}

/**
 * The rail's way to requests, after Home: Discover where the profile may
 * ask, else its requests ("Requests" for managers) where it has any to
 * see; none without requests. It carries the requests' one number.
 */
export function requestsEntry(s: RequestSummary | null): RailEntry | null {
  const { count, danger } = requestsCount(s);
  const badge = count > 0 ? { count, danger } : undefined;
  if (mayAsk(s)) return { id: "discover", label: "Discover", icon: "compass", key: { name: "discover" }, badge };
  if (hasRequests(s)) return { id: "requests", label: requestsTitle(s), icon: "inbox", key: { name: "requests" }, badge };
  return null;
}

/**
 * The rail's entries (the Android app's RailItems): Search, Home,
 * Discover (or the requests), Live TV where the server has channels for
 * this profile, up to six libraries, then Libraries when there are more,
 * Playlists, and Collections when there are six or fewer; Settings at
 * the foot. While music has a queue, Now Playing comes last. Every entry
 * is listed: those that don't fit scroll (`railScroll`).
 */
export function railEntries(libraries: Library[] | null, nowPlaying = false, live = false, requests: RailEntry | null = null): RailEntries {
  const libs = libraries ?? [];
  const icon = (l: Library): IconName => (l.kind === "movies" ? "clapperboard" : l.kind === "shows" ? "tv" : l.kind === "music" ? "music" : "film");
  const top: RailEntry[] = [
    { id: "search", label: "Search", icon: "search", key: { name: "search" } },
    { id: "home", label: "Home", icon: "house", key: { name: "home" } },
  ];
  if (requests) top.push(requests);
  if (live) top.push({ id: "live", label: "Live TV", icon: "tv-minimal", key: { name: "live" } });
  for (const l of libs.slice(0, MAX_LIBRARIES)) top.push({ id: `library:${l.id}`, label: l.name, icon: icon(l), key: { name: "library", id: l.id } });
  if (libs.length > MAX_LIBRARIES) top.push({ id: "libraries", label: "Libraries", icon: "layers", key: { name: "libraries" } });
  top.push({ id: "playlists", label: "Playlists", icon: "list-music", key: { name: "playlists" } });
  if (libs.length > 0 && libs.length <= MAX_LIBRARIES) {
    const first = libs.filter((l) => l.kind !== "music")[0] ?? libs[0]!;
    top.push({ id: "collections", label: "Collections", icon: "layers", key: { name: "collections", libraryId: first.id } });
  }
  if (nowPlaying) top.push({ id: "now-playing", label: "Now Playing", icon: "audio-lines", key: { name: "nowPlaying" } });
  return { top, settings: { id: "settings", label: "Settings", icon: "settings", key: { name: "settings" } } };
}

/**
 * The rail's entry for the page at the root of the stack (railCurrent):
 * the entry that opened it, Libraries for a library not in the rail, or
 * `null` (sign-in, or a page the rail has no entry for).
 */
export function currentEntry(root: PageKey | null, entries: RailEntries): string | null {
  if (!root) return null;
  const { top } = entries;
  switch (root.name) {
    case "library":
      return (top.find((e) => e.key.name === "library" && e.key.id === root.id) ?? top.find((e) => e.id === "libraries"))?.id ?? null;
    case "collections":
      if (top.some((e) => e.id === "collections")) return "collections";
      return root.libraryId === undefined ? null : currentEntry({ name: "library", id: root.libraryId }, entries);
    case "settings":
      return "settings";
    case "profiles":
      return "profile";
    // Discover's pages, and the requests reached from it, under whichever of the two the rail has.
    case "discover":
    case "discoverList":
    case "title":
    case "requests":
      return (top.find((e) => e.key.name === root.name) ?? top.find((e) => e.id === "discover" || e.id === "requests"))?.id ?? null;
    default:
      return top.find((e) => e.key.name === root.name)?.id ?? null;
  }
}

/** A profile's colour as CSS, or `null` when it isn't one ("#e07a5f" only). */
export function profileColor(color: string | null | undefined): string | null {
  return color && /^#[0-9a-fA-F]{6}$/.test(color) ? color : null;
}

export function initial(name: string): string {
  const c = name.trim().charAt(0);
  return c ? c.toUpperCase() : "?";
}

/**
 * The rail at the left edge: icons alone, and labels on a panel while it
 * has focus (it is open exactly then). Left from a page's first column
 * comes here, to the current entry; Right goes back to where focus was.
 */
/** What the TV says while the server is out of reach: the shell's notice, then the avatar's mark. */
export const OFFLINE_TEXT = "Showing what Pley had saved. Reconnecting…";

/** `offline`: the avatar wears the server-off mark (once the shell's notice has had its 6 s). */
export function Rail(p: { entries: RailEntries; me: User | null; current: string | null; offline?: boolean; railRef: (api: RailApi) => void }) {
  const ui = useUi();
  const open = useSignal(false);
  const root = useRef<HTMLDivElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  /** A page chosen here takes focus from the rail as soon as it has something to focus (handOver). */
  const handing = useRef(false);
  /** The pointer has been on the open rail: moving out of it closes it (a rail opened by keys stays). */
  const pointedIn = useRef(false);
  const { top, settings } = p.entries;
  /** How far the entries between the avatar and Settings are scrolled (`railScroll`). */
  const [scroll, setScroll] = useState(0);
  const entriesRef = useRef<HTMLDivElement>(null);
  const count = useRef(top.length);
  count.current = top.length;
  /** Where focus was in the rail, for the way a move went: -1 the avatar, `count` Settings; null outside it. */
  const lastAt = useRef<number | null>(null);
  /** The focused element's place: an entry's index, -1 the avatar, `count` Settings; null outside the rail. */
  const placeOf = (el: HTMLElement): number | null => {
    const r = root.current;
    if (!r || !r.contains(el)) return null;
    const entries = entriesRef.current;
    if (entries && entries.contains(el)) return Array.prototype.indexOf.call(entries.children, el) as number;
    return el.classList.contains("avatar") ? -1 : count.current;
  };

  /**
   * Focus back to the page: to where it was (Right), else to nothing in
   * particular (a page chosen here takes it). Right with nothing to go back
   * to goes into the page, and a page with nothing to focus leaves it here,
   * the rail open, as Android's drawer does (backToContent).
   */
  const leave = (restore: boolean) => {
    const el = root.current;
    if (!el) return;
    if (ui.focus.root === el) ui.focus.release(el, restore);
    const still = ui.focus.current;
    if (restore && still && el.contains(still) && !ui.focus.move("right")) {
      ui.focus.trap(el);
      return;
    }
    handing.current = false;
    pointedIn.current = false;
    open.value = false;
  };

  // A rail that goes (onto a page without it) takes its hold on focus with it.
  useLayoutEffect(() => {
    const el = root.current;
    return () => {
      if (el && ui.focus.root === el) ui.focus.release(el, false);
    };
  }, []);

  useEffect(() => {
    const api: RailApi = {
      focusCurrent: () => {
        const el = root.current;
        if (!el) return false;
        const marker = el.querySelector(`[data-rail="${p.current ?? "home"}"]`) ?? el.querySelector('[data-rail="home"]');
        const current = marker ? (marker.closest("[data-f]") as HTMLElement | null) : null;
        if (!current) return false;
        if (ui.focus.root !== el) {
          opener.current = ui.focus.current;
          ui.focus.trap(el);
        }
        ui.focus.focus(current);
        open.value = true;
        return true;
      },
      hasFocus: () => !!root.current && ui.focus.root === root.current,
      backToContent: () => leave(true),
      handOver: () => {
        if (!handing.current || !root.current || ui.focus.root !== root.current) return false;
        leave(false);
        return true;
      },
      pointTo: (target) => {
        const el = root.current;
        if (!el || !el.contains(target)) return;
        if (ui.focus.root !== el) {
          opener.current = ui.focus.current;
          ui.focus.trap(el);
        }
        ui.focus.focus(target, true);
        open.value = true;
      },
    };
    p.railRef(api);
  });

  useEffect(
    () =>
      ui.focus.onChange((el) => {
        const r = root.current;
        // Moving along the rail: the person is choosing again, and a page that loads keeps its hands off.
        if (r && el && r.contains(el)) {
          handing.current = false;
          if (ui.focus.byPointer) pointedIn.current = true;
        }
        // The focused entry in view, and the next one the way focus went; the pointer only brings a cut-off one in.
        const at = el ? placeOf(el) : null;
        const from = lastAt.current;
        lastAt.current = at;
        if (at !== null && at >= 0 && at < count.current) {
          const dir = from === null || ui.focus.byPointer || from === at ? null : from < at ? "down" : "up";
          setScroll((s) => railScroll(s, at, count.current, dir));
        }
        if (r && el && !r.contains(el) && open.value) {
          open.value = false;
          handing.current = false;
          pointedIn.current = false;
          if (ui.focus.root === r) ui.focus.release(r, false);
        }
      }),
    [],
  );

  // Entries made again (Now Playing came, a library went): the focused one stays in view, the scroll within its ends.
  useLayoutEffect(() => {
    const el = ui.focus.current;
    const at = el ? placeOf(el) : null;
    setScroll((s) => railScroll(s, at ?? -1, top.length, null));
  }, [top.length]);

  const select = (entry: RailEntry) => {
    const nav = ui.nav;
    const s = nav.stack.value;
    if (s.length === 1 && JSON.stringify(s[0]!.key) === JSON.stringify(entry.key)) {
      // The page shown already: back to where focus was on it.
      leave(false);
      if (opener.current && opener.current.isConnected) ui.focus.focus(opener.current);
      return;
    }
    // The rail keeps focus, open, until the new page has something to focus (handOver).
    handing.current = true;
    nav.restart(entry.key);
  };

  const item = (entry: RailEntry) => (
    <F
      key={entry.id}
      class={`rail-item${p.current === entry.id ? " current" : ""}`}
      label={entry.badge ? `${entry.label}, ${entry.badge.count}` : entry.label}
      onSelect={() => select(entry)}
      onKey={(key) => {
        if (key === "right") {
          leave(true);
          return true;
        }
        return key === "left";
      }}
    >
      <span data-rail={entry.id} style="display:contents" />
      <Pill />
      <Icon name={entry.icon} size={20} />
      {entry.badge && (
        <span class={`rail-badge${entry.badge.danger ? " danger" : ""}`} aria-hidden="true">
          {entry.badge.count > 99 ? "99+" : String(entry.badge.count)}
        </span>
      )}
      <span class="label t-label one-line">{entry.label}</span>
    </F>
  );

  const me = p.me;
  const color = profileColor(me?.avatar_color);
  return (
    <>
      {/* The pointer moving out of the rail over the page, or clicking the page, closes it: focus goes back to the page. */}
      <div
        class={`rail-scrim${open.value ? " in" : ""}`}
        onMouseMove={() => {
          if (open.value && pointedIn.current && ui.input.pointerMoved) leave(true);
        }}
        onClick={() => {
          if (open.value) leave(true);
        }}
      />
      <div ref={root} class={`rail${open.value ? " open" : ""}`}>
        <div class="rail-panel" />
        <div class="rail-edge" />
        <G kind="col" class="rail-items">
          <F
            class={`rail-item avatar${p.current === "profile" ? " current" : ""}`}
            label={p.offline ? `Switch profile. ${OFFLINE_TEXT}` : "Switch profile"}
            onSelect={() => select({ id: "profile", label: "Profiles", icon: "users", key: { name: "profiles" } })}
            onKey={(key) => {
              if (key === "right") {
                leave(true);
                return true;
              }
              return key === "left";
            }}
          >
            <span data-rail="profile" style="display:contents" />
            <Pill />
            <div class="rail-avatar t-label" style={color ? `background:${color}` : undefined}>
              <span style="font-weight:700">{initial(me?.name ?? "")}</span>
            </div>
            {p.offline && (
              <div class="offline-mark" aria-hidden="true">
                <Icon name="server-off" size={9} color="var(--fg)" />
              </div>
            )}
            <span class="label" style="top:6px">
              <div class="t-label one-line">{me?.name ?? ""}</div>
              <div class="t-caption one-line" style="opacity:0.8">
                Switch profile
              </div>
            </span>
          </F>
          {/* Between the avatar and Settings, clipped there: the entries scroll when there are more than fit. */}
          <div class="rail-view">
            <div ref={entriesRef} class="rail-scroll" style={scroll ? `transform:translateY(${-scroll}px)` : undefined}>
              {top.map(item)}
            </div>
          </div>
          <div class="rail-spacer" />
          {item(settings)}
        </G>
      </div>
    </>
  );
}

/** An entry's fill, which grows with the opening rail (theme.css). */
function Pill() {
  return (
    <span class="rail-pill" aria-hidden="true">
      <i class="l" />
      <i class="m" />
      <i class="r" />
    </span>
  );
}

export interface RailApi {
  /** Focus to the current entry (opening the rail); false when the rail isn't shown. */
  focusCurrent(): boolean;
  hasFocus(): boolean;
  backToContent(): void;
  /** A page chosen in the rail has something to focus: the rail lets go, true when it held focus for it. */
  handOver(): boolean;
  /** The pointer moved onto `el`: an entry of the rail opens it, focused there. */
  pointTo(el: HTMLElement): void;
}
