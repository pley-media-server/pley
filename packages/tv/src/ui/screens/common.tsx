import { createContext, type ComponentChildren } from "preact";
import { useContext, useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import { errorMessage, unreachable, unreachableReason } from "../../api/client";
import type { ItemDetail, ItemSummary } from "../../api/types";
import { SignedOutError } from "../../auth/tokens";
import { formatRuntime, localDecimal, qualityBadges } from "../../format/format";
import { t } from "../../i18n";
import { Cancel, NetworkError } from "../../net/transport";
import type { AppState } from "../../state";
import { useUi } from "../context";
import { Pill, TextButton } from "../kit/buttons";
import { G } from "../kit/focusable";
import { useTrap } from "../kit/panel";
import { Icon, type IconName } from "../kit/icons";

/** The app's state, for screens (the shell provides it with each page). */
export const AppContext = createContext<AppState | null>(null);

export function useApp(): AppState {
  const app = useContext(AppContext);
  if (!app) throw new Error("no AppState");
  return app;
}

/** What people read for a problem: trimmed, a capital first, a full stop unless it ends in one (the Android app's `sentence`). */
export function sentence(text: string): string {
  const t = text.trim();
  if (!t) return t;
  const s = t.charAt(0).toUpperCase() + t.slice(1);
  return /[.…!?]$/.test(s) ? s : `${s}.`;
}

/** Why a load failed; when the server answered nothing, what to check (the Android app's failedBody). */
export function problem(e: unknown): string {
  return unreachable(e) ? t("common.server.check", { reason: unreachableReason(e) }) : sentence(errorMessage(e));
}

/** A failed load's title: `title`, or "Can't reach your server" when the server answered nothing. */
export function failedTitle(e: unknown, title: string): string {
  return unreachable(e) ? t("common.server.cantReach") : title;
}

export interface Loaded<T> {
  data: T | null;
  error: unknown;
  loading: boolean;
  reload(): void;
}

/**
 * Data a screen shows: fetched when the screen opens and when `deps`
 * change, again after the server's changes (`state.changes`) unless
 * `live` is false. What was shown stays while a newer answer comes.
 * Answers for a screen that went away are dropped. A load the server
 * answers nothing tells the app it's out of reach; a fresh answer after
 * that failure, that it's back.
 */
export function useLoad<T>(load: (cancel: Cancel) => Promise<T>, deps: unknown[], opts: { live?: boolean; initial?: T | null } = {}): Loaded<T> {
  const app = useApp();
  const [n, setN] = useState(0);
  const changes = opts.live === false ? 0 : app.changes.value;
  const [s, setS] = useState<{ data: T | null; error: unknown; loading: boolean }>({ data: opts.initial ?? null, error: null, loading: true });
  /** The last load got no answer from the server. */
  const away = useRef(false);
  useEffect(() => {
    const cancel = new Cancel();
    setS((x) => (x.loading ? x : { ...x, loading: true }));
    load(cancel).then(
      (data) => {
        if (cancel.cancelled) return;
        if (away.current) app.serverAnswered();
        away.current = false;
        setS({ data, error: null, loading: false });
      },
      (error) => {
        if (cancel.cancelled || (error instanceof NetworkError && error.cancelled)) return;
        if (error instanceof SignedOutError) return;
        away.current = unreachable(error);
        if (away.current) app.serverAway();
        setS((x) => ({ data: x.data, error, loading: false }));
      },
    );
    return () => cancel.cancel();
  }, deps.concat([n, changes]));
  return { data: s.data, error: s.error, loading: s.loading, reload: () => setN((x) => x + 1) };
}

/** Something that happens once the page has drawn: focus a message's button after 16 ms, as the Android app does. */
export function useAfterFrame(fn: () => void, deps: unknown[]) {
  useEffect(() => {
    const t = setTimeout(fn, 16);
    return () => clearTimeout(t);
  }, deps);
}

/**
 * A message filling the page (the item page's TvMessage): an icon, a
 * title, a body, and one button that takes focus.
 */
export function Message(p: { icon?: IconName; title: string; body?: string | null; action?: { label: string; icon?: IconName; onSelect: () => void } }) {
  const ui = useUi();
  const noAction = !p.action;
  useAfterFrame(() => {
    if (noAction) ui.toRail();
  }, [noAction]);
  return (
    <div class="message-page" role="status">
      <Icon name={p.icon ?? "circle-alert"} size={40} color="var(--muted)" />
      <div class="t-page c-fg" style="margin-top:16px;text-align:center">
        {p.title}
      </div>
      {p.body && (
        <div class="t-body c-muted" style="margin-top:8px;max-width:464px;text-align:center">
          {p.body}
        </div>
      )}
      {p.action && (
        <G kind="row" style="margin-top:24px">
          <Pill label={p.action.label} icon={p.action.icon} initial onSelect={p.action.onSelect} />
        </G>
      )}
    </div>
  );
}

/**
 * A message in a page's column (the library's TvMessage): a title and a
 * body at x 96 from `top`, and maybe one button that takes focus. With
 * none, focus goes to `focusInstead`, else to the rail, so the remote is
 * never dead.
 */
export function ColumnMessage(p: {
  title: string;
  body?: string | null;
  top?: number;
  action?: { label: string; icon?: IconName; onSelect: () => void };
  focusInstead?: () => boolean;
  takeFocus?: boolean;
}) {
  const ui = useUi();
  const noAction = !p.action;
  useAfterFrame(() => {
    if (!noAction || p.takeFocus === false) return;
    const current = ui.focus.current;
    if (current && current.isConnected && ui.focus.inScope(current)) return;
    if (!(p.focusInstead && p.focusInstead())) ui.toRail();
  }, [noAction]);
  return (
    <div style={`position:absolute;left:96px;right:96px;top:${p.top ?? 140}px`} role="status">
      <div class="t-section c-fg">{p.title}</div>
      {p.body && (
        <div class="t-body c-muted" style="margin-top:8px;max-width:520px">
          {p.body}
        </div>
      )}
      {p.action && (
        <G kind="row" style="margin-top:24px">
          <Pill label={p.action.label} icon={p.action.icon} initial onSelect={p.action.onSelect} />
        </G>
      )}
    </div>
  );
}

/** A badge on a page (TileCaption's): an Outline frame, Muted text. */
export function PageBadge(p: { text: string }) {
  return <span class="page-badge t-badge">{p.text}</span>;
}

/**
 * The meta line (TvMetaLine): the lead (an episode's context), year,
 * content rating, runtime, the rating with a star, quality badges, "·"
 * between the words.
 */
export function MetaLine(p: { item: ItemSummary; detail?: ItemDetail | null; lead?: string | null; runtime?: string | null; badges?: boolean; maxWidth?: number }) {
  const parts: ComponentChildren[] = [];
  const sep = (key: string) => (
    <span key={key} class="t-label c-subtle meta-sep">
      ·
    </span>
  );
  const push = (key: string, node: ComponentChildren, separated = true) => {
    if (parts.length && separated) parts.push(sep(`sep-${key}`));
    parts.push(node);
  };
  if (p.lead) push("lead", <span key="lead" class="t-label c-fg one-line meta-lead">{p.lead}</span>);
  const item = p.item;
  if (item.year) push("year", <span key="year" class="t-label c-muted">{item.year}</span>);
  const rating = p.detail?.content_rating;
  if (rating) push("cr", <PageBadge key="cr" text={rating} />);
  const runtime = p.runtime !== undefined ? p.runtime : item.kind === "movie" || item.kind === "episode" ? formatRuntime(item.duration_ms) : "";
  if (runtime) push("rt", <span key="rt" class="t-label c-muted nowrap">{runtime}</span>);
  if (item.rating && item.rating > 0) {
    const rated = localDecimal(item.rating.toFixed(1));
    push(
      "rating",
      <span key="rating" class="meta-rating" aria-label={t("browse.rated", { rating: rated })}>
        <Icon name="star" size={14} color="var(--accent)" />
        <span class="t-label c-fg" style="margin-left:4px">
          {rated}
        </span>
      </span>,
    );
  }
  if (p.badges !== false && p.detail) {
    const badges = qualityBadges(p.detail.files[0] ?? null);
    if (badges.length) {
      push(
        "badges",
        <span key="badges" class="meta-badges">
          {badges.map((b) => (
            <PageBadge key={b} text={b} />
          ))}
        </span>,
      );
    }
  }
  return (
    <div class="meta-line" style={`max-width:${p.maxWidth ?? 464}px`}>
      {parts}
    </div>
  );
}

/**
 * A dialog over the page (TvDialog): a title, a body, buttons at the
 * right; Back, or the pointer's click outside it, dismisses it. Focus
 * stays inside it.
 */
export function Dialog(p: {
  title: string;
  body?: ComponentChildren;
  buttons: { label: string; onSelect: () => void; color?: string; initial?: boolean }[];
  onDismiss: () => void;
}) {
  const ui = useUi();
  const trap = useTrap();
  useLayoutEffect(
    () =>
      ui.input.push((key, e) => {
        if (key !== "back") return false;
        if (e.type === "down" && !e.repeat) p.onDismiss();
        return true;
      }),
    [],
  );
  return (
    <div ref={trap} class="dialog-root" role="dialog" aria-label={p.title}>
      <div class="scrim in" onClick={p.onDismiss} />
      <div class="dialog small">
        <div class="t-title c-fg">{p.title}</div>
        {p.body && <div class="t-body c-muted" style="margin-top:8px">{p.body}</div>}
        <G kind="row" class="dialog-buttons">
          {p.buttons.map((b) => (
            <TextButton key={b.label} label={b.label} color={b.color} initial={b.initial} onSelect={b.onSelect} />
          ))}
        </G>
      </div>
    </div>
  );
}

/** A page title at the top of a column (x 96, y 27). */
export function PageTitle(p: { children: ComponentChildren; top?: number }) {
  return (
    <div class="t-page c-fg one-line" style={`position:absolute;left:96px;right:48px;top:${p.top ?? 27}px`}>
      {p.children}
    </div>
  );
}

/** Errors that mean "try again later" rather than "this won't work". */
export function isNetwork(e: unknown): boolean {
  return e instanceof NetworkError;
}
