import { useSignal, useSignalEffect } from "@preact/signals";
import { useEffect, useLayoutEffect, useMemo, useRef } from "preact/hooks";
import type { AppState } from "../../state";
import { networkMayHaveChanged } from "../../auth/tokens";
import { UiContext, type Ui } from "../context";
import { FocusManager } from "../focus";
import { ImageLoader } from "../images";
import { Input } from "../input";
import { initMusic } from "../../player/music";
import { AppContext, Dialog } from "../screens/common";
import { Screen } from "../screens/screens";
import { Icon } from "../kit/icons";
import { Toast } from "../kit/toast";
import { castCommand } from "../../cast";
import { castOpening, Navigator, type Entry, type PageKey } from "./navigator";
import { RequestNotices, type NoticeApi } from "./notice";
import { currentEntry, OFFLINE_TEXT, Rail, railEntries, requestsEntry, type RailApi } from "./rail";

/** Pages without the rail: sign-in, "Who's watching?" and the players. */
function railless(key: PageKey | undefined): boolean {
  return !key || key.name === "servers" || key.name === "link" || key.name === "profiles" || isPlayer(key);
}

function isPlayer(key: PageKey | undefined): boolean {
  return !!key && (key.name === "player" || key.name === "livePlayer");
}

export interface ShellProps {
  state: AppState;
  /** Pixels per canvas px on this screen. */
  scale: number;
}

/**
 * The app: one page at a time from the back stack, the rail beside the
 * pages that have it, the screen-change curtain, toasts, and the remote's
 * Back by the Android app's order: what is open on top first, then the
 * rail exits, a pushed page pops, a root page opens the rail.
 */
export function App(p: ShellProps) {
  const state = p.state;
  const focus = useMemo(() => new FocusManager(), []);
  const input = useMemo(() => new Input(focus, (e) => state.platform.mapKey(e)), []);
  const nav = useMemo(() => new Navigator(), []);
  const music = useMemo(() => initMusic(state), []);
  const rail = useRef<RailApi | null>(null);
  const notice = useRef<NoticeApi | null>(null);
  const toast = useSignal<{ text: string; error: boolean; id: number } | null>(null);
  const curtain = useSignal(0);
  const exiting = useSignal(false);
  const pageRoot = useRef<HTMLDivElement>(null);
  const api = state.api.value;
  const tier = useMemo(() => state.platform.tier(), []);
  const images = useMemo(
    () => (api ? new ImageLoader(api, { concurrency: tier === "low" ? 4 : 6, cap: tier === "low" ? 1280 : p.scale * 960 >= 1900 ? 1920 : 1280 }) : null),
    [api],
  );
  const reduced = state.settings.value.reduceMotion;
  /** When the page on top was shown, and whether it got its remembered element back. */
  const shownAt = useRef(0);
  const restored = useRef(new Set<number>());

  // The first page, from what the TV kept: no network before the first frame.
  if (!nav.stack.value.length) {
    const start = state.start();
    nav.restart(start.page === "link" ? { name: "link", server: start.server } : start.page === "pin" ? { name: "profiles", pin: true } : { name: start.page });
  }

  // A sign-in the server ended: back to signing in.
  useSignalEffect(() => {
    if (state.signedOut.value && state.server.value) {
      const top = nav.top?.key;
      if (top?.name !== "link" && top?.name !== "servers") nav.restart({ name: "link", server: state.server.value });
    }
  });

  const toRail = () => !railless(nav.root?.key) && !isPlayer(nav.top?.key) && !!rail.current?.focusCurrent();
  // What plays (the music) ends on the server first, a second at most: its session would otherwise be kept 90 s.
  const leaveApp = () => {
    state.endPlayback();
    void state.reporter.flush(1_000).then(() => state.platform.exit());
  };
  const showToast = (text: string, error = false) => (toast.value = { text, error, id: Date.now() });
  const claim = (el: HTMLElement, fid: string | null, initial: boolean) => {
    const entry = nav.top;
    if (!entry) return;
    // A page chosen in the rail takes focus from it once it has something to focus; until then the rail keeps it, open.
    if (initial && pageRoot.current && pageRoot.current.contains(el)) rail.current?.handOver();
    if (!focus.inScope(el)) return;
    // The person moved to the rail while the page loaded: it keeps focus.
    if (rail.current?.hasFocus() && focus.root !== el.closest(".panel-root")) return;
    const current = focus.current;
    const scopeHasFocus = !!current && current.isConnected && focus.inScope(current);
    const touched = input.lastKeyAt > shownAt.current;
    const remembering = !!entry.memory && !touched && !restored.current.has(entry.id);
    if (remembering && fid === entry.memory) {
      restored.current.add(entry.id);
      focus.focus(el);
      return;
    }
    if (!initial || scopeHasFocus) return;
    if (remembering) {
      // Something else is remembered: it gets a moment to come back first.
      setTimeout(() => {
        const now = focus.current;
        if (!(now && now.isConnected && focus.inScope(now)) && el.isConnected) focus.focus(el);
      }, 60);
      return;
    }
    focus.focus(el);
  };
  const ui: Ui = {
    input,
    focus,
    nav,
    api: api as Ui["api"],
    images: images as Ui["images"],
    scale: p.scale,
    tier,
    reducedMotion: reduced,
    toast: showToast,
    toRail,
    claim,
  };

  // Keys, Back last of all.
  useEffect(() => {
    const detach = input.attach(document);
    input.onMoveFailed = (dir) => {
      // Left with nothing further left, anywhere on a page, goes to the rail.
      if (dir === "left" && !rail.current?.hasFocus() && focus.root === pageRoot.current) toRail();
    };
    const exit = () => {
      if (state.platform.asksBeforeExit) exiting.value = true;
      else leaveApp();
    };
    const off = input.push((key, e) => {
      if (key !== "back") return false;
      if (e.type !== "down" || e.repeat) return true;
      if (rail.current?.hasFocus()) {
        exit();
        return true;
      }
      if (nav.pop()) return true;
      if (toRail()) return true;
      exit();
      return true;
    });
    state.platform.registerKeys?.();
    return () => {
      off();
      detach();
    };
  }, []);

  // A title cast from a phone or a browser plays from whatever page shows, once the app is in use; a video or channel playing leaves first, as on Back.
  useEffect(
    () =>
      state.onEvent((e) => {
        if (e.type !== "cast_play" || !state.signedIn || state.signedOut.value) return;
        const cast = castCommand(e, state.castIdentity().id);
        if (cast?.kind !== "play") return;
        const opening = castOpening(nav.stack.value);
        if (!opening) return;
        exiting.value = false;
        const key: PageKey = {
          name: "player",
          request: { itemId: cast.itemId, fileId: cast.fileId, startMs: cast.positionMs },
          pressedAt: typeof performance !== "undefined" ? performance.now() : Date.now(),
        };
        if (opening === "replace") nav.replace(key);
        else nav.open(key);
      }),
    [],
  );

  // A held direction key: rows and tiles move without animating. Told as keys come, so an idle TV runs no timer.
  useEffect(() => {
    input.onHolding = (holding) => document.getElementById("app")?.classList.toggle("holding", holding);
    input.onPointOutside = (el) => {
      // A notice holding focus closes as the pointer moves onto the page; the rail opens for it as ever.
      notice.current?.pointedAway(el);
      rail.current?.pointTo(el);
    };
    return () => {
      input.onHolding = null;
      input.onPointOutside = null;
    };
  }, []);

  useEffect(() => {
    document.getElementById("app")?.classList.toggle("reduced", reduced);
  }, [reduced]);

  // The TV's screen reader reads the element the browser has focused: while it is on, that follows the app's focus.
  useEffect(() => {
    state.platform.screenReader?.((on) => (focus.dom = on));
  }, []);

  // Page focus memory: what the page focused last, so Back lands on it. A panel's or a dialog's rows
  // leave it on what opened them: an action that leaves the page (Play from here, Go to) comes back there.
  useEffect(
    () =>
      focus.onChange((el) => {
        const entry = nav.top;
        const root = pageRoot.current;
        if (!entry || !el || !root || !root.contains(el) || focus.root !== root) return;
        entry.memory = el.getAttribute("data-fid");
      }),
    [],
  );

  // Who is watching and the libraries, once signed in; the events stream while the app shows.
  useEffect(() => {
    if (api && state.signedIn) void state.loadAccount().catch(() => undefined);
  }, [api]);
  useEffect(() => {
    state.platform.onVisibility((visible) => {
      // Back from the TV's other apps or standby, maybe on another network.
      if (visible) networkMayHaveChanged();
      state.visible.value = visible;
      state.listen(visible);
    });
    window.addEventListener("online", networkMayHaveChanged);
  }, []);
  useEffect(() => {
    state.listen(state.visible.value);
    return () => state.listen(false);
  }, [api, state.signedOut.value]);

  const stack = nav.stack.value;
  const top = stack[stack.length - 1] as Entry | undefined;
  const lastTop = useRef<number | null>(null);
  /**
   * The page's root, as soon as it exists: before the page's own elements
   * claim focus (refs are set before any layout effect runs).
   */
  const attachPage = (el: HTMLDivElement | null) => {
    pageRoot.current = el;
    if (!el) return;
    focus.setPage(el);
    const id = Number(el.getAttribute("data-entry"));
    if (lastTop.current === id) return;
    shownAt.current = Date.now();
    restored.current.delete(id);
    // Focus left over from the page before is no use here; the rail keeps its own while a page it chose loads.
    if (focus.current && !el.contains(focus.current) && !rail.current?.hasFocus()) focus.blur();
  };
  useLayoutEffect(() => {
    if (!top) return;
    if (lastTop.current !== null && lastTop.current !== top.id && !reduced) curtain.value++;
    lastTop.current = top.id;
  }, [top?.id]);

  useEffect(() => {
    if (!toast.value) return;
    const t = setTimeout(() => (toast.value = null), toast.value.error ? 6000 : 3500);
    return () => clearTimeout(t);
  }, [toast.value?.id]);

  const showRail = !railless(stack[0]?.key) && !isPlayer(top?.key) && !!api;
  const libraries = state.libraries.value;
  const playing = music.now.value !== null;
  const live = !!state.live.value?.available;
  // Discover for whoever may ask, else their requests; with the one number that waits there.
  const requests = requestsEntry(state.requests.value);
  const entries = useMemo(
    () => railEntries(libraries, playing, live, requests),
    [libraries, playing, live, requests?.id ?? null, requests?.badge?.count ?? 0, requests?.badge?.danger ?? false],
  );
  // Ready-to-watch news pops up only where nothing else holds focus: not over a video, a panel, a dialog, the rail or typing.
  const noticeFree = () => !!pageRoot.current && focus.root === pageRoot.current && !rail.current?.hasFocus() && !input.typing && !exiting.value;
  return (
    <AppContext.Provider value={state}>
      <UiContext.Provider value={ui}>
        <div ref={attachPage} class="screen" key={top?.id} data-entry={top?.id}>
          {top && <Screen entry={top} state={state} />}
        </div>
        {showRail && (
          <Rail
            entries={entries}
            me={state.me.value}
            current={currentEntry(stack[0]?.key ?? null, entries)}
            offline={state.offline.value && !state.offlineNotice.value}
            railRef={(r) => (rail.current = r)}
          />
        )}
        {showRail && state.offlineNotice.value && (
          // Where toasts are; never focusable, as there is nothing to do but wait.
          <div class="offline-notice t-label" role="status" aria-live="polite">
            <Icon name="server-off" size={20} color="var(--muted)" />
            <span>{OFFLINE_TEXT}</span>
          </div>
        )}
        {showRail && top?.key.name !== "requests" && (
          <RequestNotices
            state={state}
            free={noticeFree}
            noticeRef={(n) => (notice.current = n)}
            onOpen={(n) => (n.itemId !== null ? nav.open({ name: "item", id: n.itemId }) : nav.open({ name: "requests" }))}
          />
        )}
        {exiting.value && (
          <Dialog
            title="Exit Pley?"
            onDismiss={() => (exiting.value = false)}
            buttons={[
              { label: "Cancel", onSelect: () => (exiting.value = false) },
              { label: "Exit", initial: true, onSelect: leaveApp },
            ]}
          />
        )}
        {curtain.value > 0 && <div key={curtain.value} class="curtain fade" />}
        <Toast toast={toast.value} />
      </UiContext.Provider>
    </AppContext.Provider>
  );
}
