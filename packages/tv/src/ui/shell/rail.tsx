import { useSignal } from "@preact/signals";
import { useEffect, useLayoutEffect, useRef } from "preact/hooks";
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

/** At most this many libraries in the rail. */
const MAX_LIBRARIES = 3;
/** Entries between the avatar and Settings, at most: their pitch fills the rail. */
const MAX_ITEMS = 7;

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
 * this profile, up to three libraries, then Libraries when there are more,
 * Playlists, and Collections when there are three or fewer; Settings at
 * the foot. While music has a queue, Now Playing comes last. Seven fit:
 * past that, Collections goes first, then the libraries from the last,
 * Libraries listing them.
 */
export function railEntries(libraries: Library[] | null, nowPlaying = false, live = false, requests: RailEntry | null = null): RailEntries {
  const libs = libraries ?? [];
  const icon = (l: Library): IconName => (l.kind === "movies" ? "clapperboard" : l.kind === "shows" ? "tv" : l.kind === "music" ? "music" : "film");
  let shown = Math.min(libs.length, MAX_LIBRARIES);
  let collections = libs.length > 0 && libs.length <= MAX_LIBRARIES;
  const build = (): RailEntry[] => {
    const top: RailEntry[] = [
      { id: "search", label: "Search", icon: "search", key: { name: "search" } },
      { id: "home", label: "Home", icon: "house", key: { name: "home" } },
    ];
    if (requests) top.push(requests);
    if (live) top.push({ id: "live", label: "Live TV", icon: "tv-minimal", key: { name: "live" } });
    for (const l of libs.slice(0, shown)) top.push({ id: `library:${l.id}`, label: l.name, icon: icon(l), key: { name: "library", id: l.id } });
    if (libs.length > shown) top.push({ id: "libraries", label: "Libraries", icon: "layers", key: { name: "libraries" } });
    top.push({ id: "playlists", label: "Playlists", icon: "list-music", key: { name: "playlists" } });
    if (collections) {
      const first = libs.filter((l) => l.kind !== "music")[0] ?? libs[0]!;
      top.push({ id: "collections", label: "Collections", icon: "layers", key: { name: "collections", libraryId: first.id } });
    }
    if (nowPlaying) top.push({ id: "now-playing", label: "Now Playing", icon: "audio-lines", key: { name: "nowPlaying" } });
    return top;
  };
  let top = build();
  while (top.length > MAX_ITEMS && (collections || shown > 0)) {
    if (collections) collections = false;
    else shown--;
    top = build();
  }
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
        if (r && el && !r.contains(el) && open.value) {
          open.value = false;
          handing.current = false;
          pointedIn.current = false;
          if (ui.focus.root === r) ui.focus.release(r, false);
        }
      }),
    [],
  );

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
            <span class="label" style="top:10px">
              <div class="t-label one-line">{me?.name ?? ""}</div>
              <div class="t-caption one-line" style="opacity:0.8">
                Switch profile
              </div>
            </span>
          </F>
          {top.map(item)}
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
