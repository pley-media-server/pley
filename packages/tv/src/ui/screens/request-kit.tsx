import type { ComponentChildren } from "preact";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import type { DiscoverDetail, DiscoverTitle, ItemKind, ItemRequest, MediaRequest, NewRequest } from "../../api/types";
import { displayAddress } from "../../auth/addresses";
import {
  actionDone,
  allowanceLines,
  askedText,
  askRefusal,
  listAction,
  mainAction,
  mayAsk,
  moving,
  NO_ACCESS,
  problemWords,
  requestsProblem,
  requestsTitle,
  SIGN_IN_BODY,
  stageWords,
  titleKey,
  titleRequest,
  type MainAction,
  type RequestsProblem,
  type StageWords,
  type Tone,
} from "../../requests";
import type { AppState } from "../../state";
import { useUi, type Ui } from "../context";
import { Art } from "../kit/art";
import { Pill, Round } from "../kit/buttons";
import { F, G } from "../kit/focusable";
import { Icon, type IconName } from "../kit/icons";
import { memo } from "../kit/memo";
import { TileCaption, TILES } from "../kit/tile";
import { Dialog, failedTitle, Message, problem, useAfterFrame, useApp, useLoad } from "./common";

/*
 * What Discover, a title's page, the requests and the libraries' pages
 * share of requests: tiles with a title's badge or a request's stage, the
 * stage pill and download bar, a title's main action with what it says
 * under it, asking, and the pages that say why requests can't be used.
 */

/** Whether the TV tells the time in 24 hours; one that can't say doesn't. */
export function hours24(app: AppState): boolean {
  try {
    return app.platform.clock24();
  } catch {
    return false;
  }
}

/** Where a request has got to, in words, for this sign-in (managers read why one failed). */
export function stageOf(app: AppState, r: MediaRequest): StageWords {
  return stageWords(r, { manage: !!app.requests.value?.manage, hours24: hours24(app) });
}

/** A request's stage: a dot and the words in its colour (waiting amber, on its way blue, ready green, refused red, gone grey). */
export function StagePill(p: { words: StageWords; small?: boolean }) {
  return (
    <span class={`stage-pill tone-${p.words.tone}${p.small ? " small" : ""}`}>
      <i class="stage-dot" />
      <span class={`${p.small ? "t-caption" : "t-label"} one-line`}>{p.words.text}</span>
    </span>
  );
}

/** How much of a download is in, 0–1. */
export function DownloadBar(p: { progress: number; width?: number }) {
  const pct = Math.max(0, Math.min(1, p.progress)) * 100;
  return (
    <div class="download-bar" style={p.width ? `width:${p.width}px` : undefined} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)}>
      <div style={`width:${pct.toFixed(1)}%`} />
    </div>
  );
}

/** A stage with a download's bar under it, as wide as its pill: beside a title's button, at a card's end. */
export function StageBlock(p: { words: StageWords }) {
  const bar = p.words.progress !== null;
  return (
    <div class={`stage-block${bar ? " with-bar" : ""}`}>
      <StagePill words={p.words} />
      {bar && <DownloadBar progress={p.words.progress!} />}
    </div>
  );
}

/** A request's stage under a library title's facts: the pill, and a download's bar beside it. */
export function ItemRequestLine(p: { request: MediaRequest }) {
  const app = useApp();
  const words = stageOf(app, p.request);
  return (
    <div class="item-request" aria-label={words.text}>
      <StagePill words={words} small />
      {words.progress !== null && <DownloadBar progress={words.progress} width={120} />}
    </div>
  );
}

/**
 * A title from TMDB as a poster: its badge ("In library", "Requested"…)
 * on the picture in the badge's colour, its name and year under it.
 * Props are plain values, so a row drawn again draws only the tiles whose
 * badge changed.
 */
export const TitleTile = memo(TitleTileView, ["priority"]);

function TitleTileView(p: {
  name: string;
  subtitle: string;
  poster: string | null;
  badge: string | null;
  tone: Tone;
  fid: string;
  initial?: boolean;
  priority?: number;
  onFocus?: (el: HTMLElement) => void;
  onSelect: () => void;
}) {
  const spec = TILES.poster;
  return (
    <F
      class="tile"
      style={`width:${spec.w}px`}
      fid={p.fid}
      initial={p.initial}
      label={[p.name, p.subtitle, p.badge].filter(Boolean).join(", ")}
      onSelect={p.onSelect}
      onFocus={p.onFocus}
    >
      <div class="tile-well" style={`width:${spec.w}px;height:${spec.h}px`}>
        <div class="tile-clip">
          <Art candidates={p.poster ? [p.poster] : []} width={spec.w} title={p.name} showTitle class="tile-clip" priority={p.priority} />
          <div class="tile-hairline" />
          {p.badge && <div class={`title-badge t-badge tone-${p.tone}`}>{p.badge}</div>}
        </div>
        <div class="tile-ring" />
      </div>
      <TileCaption title={p.name} subtitle={p.subtitle} />
    </F>
  );
}

/** A request as a wide tile: its picture, a download's progress on it, the title, and its stage in the stage's colour. */
export const RequestTile = memo(RequestTileView, ["priority"]);

function RequestTileView(p: {
  name: string;
  art: string[];
  stage: string;
  tone: Tone;
  progress: number | null;
  /** News since Your requests was last looked at. */
  unseen: boolean;
  fid: string;
  initial?: boolean;
  priority?: number;
  onFocus?: (el: HTMLElement) => void;
  onSelect: () => void;
}) {
  const spec = TILES.landscape;
  return (
    <F class="tile" style={`width:${spec.w}px`} fid={p.fid} initial={p.initial} label={`${p.name}, ${p.stage}`} onSelect={p.onSelect} onFocus={p.onFocus}>
      <div class="tile-well" style={`width:${spec.w}px;height:${spec.h}px`}>
        <div class="tile-clip">
          <Art candidates={p.art} width={spec.w} title={p.name} showTitle class="tile-clip" priority={p.priority} />
          <div class="tile-hairline" />
          {p.progress !== null && (
            <div class="tile-progress">
              <div style={`width:${(Math.max(0, Math.min(1, p.progress)) * 100).toFixed(1)}%`} />
            </div>
          )}
          {p.unseen && <div class="tile-news" />}
        </div>
        <div class="tile-ring" />
      </div>
      <div class="tile-caption" aria-hidden="true">
        <div class="title t-caption one-line">{p.name}</div>
        <div class={`sub t-caption one-line tone-text-${p.tone}`}>{p.stage}</div>
      </div>
    </F>
  );
}

/**
 * Asks for a title, or follows the requests that already cover it (Tell
 * me too): a toast says how it went or why not, and what shows requests
 * asks again. `null` when it was refused.
 */
export async function askFor(app: AppState, ui: Ui, body: NewRequest): Promise<{ request: MediaRequest; created: boolean } | null> {
  try {
    const r = await ui.api.askFor(body);
    ui.toast(askedText(r.request, r.created));
    app.requestsChanged(titleKey(body));
    void app.loadRequests(true);
    return r;
  } catch (e) {
    const p = requestsProblem(e);
    ui.toast(askRefusal(p, body.kind, app.requests.value), true);
    // What may be asked for, or whether at all, changed meanwhile.
    void app.loadRequests(true);
    if (p.kind === "conflict" || p.kind === "declined") app.requestsChanged(titleKey(body));
    return null;
  }
}

/** Stops following someone else's request. */
export async function stopFollowing(app: AppState, ui: Ui, r: MediaRequest): Promise<boolean> {
  try {
    await ui.api.unwatchRequest(r.id);
    ui.toast(actionDone("unwatch", r));
    app.requestsChanged(titleKey(r));
    void app.loadRequests(true);
    return true;
  } catch (e) {
    ui.toast(askRefusal(requestsProblem(e), r.kind, null), true);
    return false;
  }
}

/**
 * Titles' badges as request events left them since a page drew them
 * (`at`): a page that draws badges watches the events' marks while it
 * shows.
 */
export function useMarks(): (t: DiscoverTitle, at: number) => DiscoverTitle {
  const app = useApp();
  useEffect(() => app.watchMarks(), []);
  const marks = app.marks.value;
  return (t, at) => {
    const m = marks[titleKey(t)];
    return m && m.at > at ? { ...t, status: m.status, item_id: m.item_id ?? t.item_id } : t;
  };
}

/** A title's details came: rows drawn before show its badge as they say. */
export function keepTitle(app: AppState, d: DiscoverDetail): void {
  const m = app.marks.value[titleKey(d)];
  if (!m || m.status !== d.status || m.item_id !== d.item_id) app.markTitle(d.kind, d.tmdb_id, d.status, d.item_id);
}

/** Calls `reload` every `ms` while the app shows (`null`: not at all). */
export function usePoll(reload: () => void, ms: number | null): void {
  const app = useApp();
  const latest = useRef(reload);
  latest.current = reload;
  const visible = app.visible.value;
  useEffect(() => {
    if (ms === null || !visible) return;
    const t = setInterval(() => latest.current(), ms);
    return () => clearInterval(t);
  }, [ms, visible]);
}

/**
 * A library title's request, for a movie or show: asked once its page
 * has drawn, never before, and only where the profile may ask or has asked
 * before; again when requests change for the title. `null` while unknown,
 * and for a title TMDB doesn't know.
 */
export function useItemRequest(itemId: number, kind: ItemKind): ItemRequest | null {
  const app = useApp();
  const ui = useUi();
  const s = app.requests.value;
  const wanted = (kind === "movie" || kind === "show") && !!s && s.enabled && s.sign_in_again !== "requests" && (mayAsk(s) || s.own > 0);
  const [drawn, setDrawn] = useState(false);
  useAfterFrame(() => setDrawn(true), []);
  // The title's key once the server named it, so that its own changes ask again.
  const key = useRef("");
  const changes = useTitleChanges(key.current);
  const loaded = useLoad((c) => (wanted && drawn ? ui.api.itemRequest(itemId, c) : Promise.resolve(null)), [itemId, wanted, drawn, changes]);
  const r = wanted ? loaded.data : null;
  if (r) key.current = titleKey(r);
  // Its download moves without events: asked again every 10 s while it does.
  usePoll(loaded.reload, r && r.request && moving([r.request]) ? 10_000 : null);
  return r;
}

/** Moves when requests changed for this title (`kind:tmdb_id`), or for many: what shows its requests asks again. */
export function useTitleChanges(key: string): number {
  const app = useApp();
  const change = app.requestChanges.value;
  const seen = useRef(change.n);
  const mine = useRef(change.n);
  if (change.n !== seen.current) {
    seen.current = change.n;
    if (change.title === null || change.title === key) mine.current = change.n;
  }
  return mine.current;
}

/**
 * A title's one main action as buttons, with the stage and a
 * download's bar beside [Requested] or [Following], then what it says
 * under them: the stage's line, the allowance (for the title's kind) when
 * nothing is left to ask for, why it can't be asked for. Focus that was
 * on a button that went (the title was just asked for) goes to the row's
 * first.
 */
export function TitleActions(p: {
  /** The details; `null` while they load (the list's title stands in, never with Tell me too). */
  detail: DiscoverDetail | null;
  title: DiscoverTitle;
  /** Asking for a show: its seasons are picked first. */
  onPick: () => void;
  fid: (name: string) => string;
  initial?: boolean;
  onFocus?: () => void;
  /** Buttons after the main action's; `initial`: none of the main action's takes focus first, so one of these does. */
  extra?: (initial: boolean) => ComponentChildren;
  /** Discover's hero, which has room for little: the first of what it says, above the buttons. */
  compact?: boolean;
  class?: string;
}) {
  const app = useApp();
  const ui = useUi();
  const s = app.requests.value;
  const d = p.detail;
  const t = d ?? p.title;
  const action: MainAction = d ? mainAction(d, s) : listAction(p.title, s);
  const busy = useRef(false);
  const [signIn, setSignIn] = useState(false);
  const group = useRef<HTMLElement | null>(null);
  const hadFocus = useRef(false);

  useEffect(
    () =>
      ui.focus.onChange((el) => {
        if (el && el.isConnected) hadFocus.current = !!group.current && group.current.contains(el);
      }),
    [],
  );
  // The focused button went (Request became Requested and the row changed): the row's first, else on down.
  useLayoutEffect(() => {
    const at = ui.focus.current;
    if (!hadFocus.current || (at && at.isConnected)) return;
    hadFocus.current = false;
    const first = group.current?.querySelector<HTMLElement>("[data-f]:not([data-fdisabled])");
    if (!(first && ui.focus.focus(first))) ui.focus.move("down");
  });

  const ask = (body: NewRequest) => {
    if (busy.current) return;
    busy.current = true;
    void askFor(app, ui, body).then(() => (busy.current = false));
  };
  const focus = () => p.onFocus?.();
  const request = d ? titleRequest(d) : null;
  const stage = request && (action.kind === "requested" || action.kind === "following") ? stageOf(app, request) : null;
  const buttons: ComponentChildren[] = [];
  /** After the stage pill: Stop following. */
  const after: ComponentChildren[] = [];
  const notes: { icon?: IconName; text: string; tone?: "danger" }[] = [];
  /** A button of the main action's takes focus first. */
  let claimed = false;
  const allowance = () => {
    // The details' own count when there are details (no second call), the summary's limits.
    const left = d ?? s;
    const lines = allowanceLines({
      movies_left: left?.movies_left ?? null,
      seasons_left: left?.seasons_left ?? null,
      movies_next_at: left?.movies_next_at ?? null,
      seasons_next_at: left?.seasons_next_at ?? null,
      movies_limit: s?.movies_limit ?? null,
      seasons_limit: s?.seasons_limit ?? null,
      kind: t.kind,
    });
    for (const line of lines ?? []) notes.push({ text: line });
  };
  const main = (label: string, icon: IconName, onSelect: (() => void) | undefined, disabled = false) => {
    const initial = !!p.initial && !disabled;
    claimed = claimed || initial;
    buttons.push(<Pill key="main" label={label} icon={icon} fid={p.fid("main")} initial={initial} disabled={disabled} onFocus={focus} onSelect={onSelect} />);
  };

  switch (action.kind) {
    case "signIn":
      main("Sign in again", "user-round", () => setSignIn(true));
      notes.push({ text: SIGN_IN_BODY });
      break;
    case "restricted":
      notes.push({ icon: "lock", text: action.text });
      break;
    case "open": {
      const id = action.itemId;
      main("Open", "play", id !== null ? () => ui.nav.open({ name: "item", id }) : undefined, id === null);
      break;
    }
    case "declined":
      notes.push({ icon: "ban", text: action.text, tone: "danger" });
      break;
    case "requested":
      // Inert: it says where things are; the stage beside it says how far.
      main("Requested", "check", () => undefined);
      break;
    case "following": {
      main("Following", "bell", () => undefined);
      const r = action.request;
      if (r && r.actions.indexOf("unwatch") >= 0) {
        after.push(
          <Round
            key="unwatch"
            label="Stop following"
            icon="bell-off"
            fid={p.fid("unwatch")}
            onFocus={focus}
            onSelect={() => {
              if (busy.current) return;
              busy.current = true;
              void stopFollowing(app, ui, r).then(() => (busy.current = false));
            }}
          />,
        );
      }
      break;
    }
    case "tellMeToo":
      main("Tell me too", "bell", () => ask({ kind: t.kind, tmdb_id: t.tmdb_id, seasons: action.seasons && action.seasons.length ? action.seasons : undefined }));
      notes.push({ text: "Someone asked for it already." });
      break;
    case "more": {
      const id = action.itemId;
      if (id !== null) main("Open", "play", () => ui.nav.open({ name: "item", id }));
      if (action.more && action.ask) {
        const initial = !!p.initial && id === null && !action.disabled;
        claimed = claimed || initial;
        buttons.push(
          <Pill key="more" label="Request more seasons" icon="plus" fid={p.fid("more")} initial={initial} disabled={action.disabled} onFocus={focus} onSelect={p.onPick} />,
        );
        if (action.disabled) allowance();
      } else if (!action.ask) notes.push({ text: NO_ACCESS });
      break;
    }
    case "request":
      main("Request", "plus", () => (t.kind === "movie" ? ask({ kind: "movie", tmdb_id: t.tmdb_id }) : p.onPick()), action.disabled);
      if (action.disabled) allowance();
      break;
    case "noAccess":
      notes.push({ text: NO_ACCESS });
      break;
  }

  // The stage's own line comes first: it says more of the pill beside the button.
  if (stage && stage.note) notes.unshift({ text: stage.note });
  const said = p.compact ? notes.slice(0, 1) : notes;
  // With no button of the main action's, what it says stands in its place, over the other buttons.
  const above = p.compact || !buttons.length;
  const words = said.length ? (
    <div class={`title-notes t-body${above ? " above" : ""}`}>
      {said.map((n, i) => (
        <div key={i} class={`title-note${n.tone === "danger" ? " danger" : ""}`}>
          {n.icon && <Icon name={n.icon} size={16} />}
          <span class="lines-2">{keepLastWords(n.text)}</span>
        </div>
      ))}
    </div>
  ) : null;
  return (
    <>
      {above && words}
      <G kind="row" restore class={`buttons ${p.class ?? ""}`} elRef={(el) => (group.current = el)}>
        {buttons}
        {stage && <StageBlock key="stage" words={stage} />}
        {after}
        {p.extra?.(!!p.initial && !claimed)}
      </G>
      {!above && words}
      {signIn && <SignInAgainDialog onClose={() => setSignIn(false)} />}
    </>
  );
}

const NO_BREAK_SPACE = String.fromCharCode(0xa0);

/** A line's last two words kept together, so a wrapped line never ends with one word alone ("… in 3" / "days."). */
export function keepLastWords(text: string): string {
  const at = text.lastIndexOf(" ");
  return at > 0 ? text.slice(0, at) + NO_BREAK_SPACE + text.slice(at + 1) : text;
}

/** Signing in again, so the TV may ask (or decide): signed out here, and a new code to link it. */
export function SignInAgainDialog(p: { onClose: () => void }) {
  const app = useApp();
  const ui = useUi();
  const server = app.server.value;
  return (
    <Dialog
      title="Sign in again?"
      body={`This TV signs out of ${server ? displayAddress(server) : "the server"} and shows a code to link it again.`}
      onDismiss={p.onClose}
      buttons={[
        { label: "Cancel", onSelect: p.onClose },
        {
          label: "Sign in again",
          initial: true,
          onSelect: () => {
            p.onClose();
            void app.signOut().then(() => ui.nav.restart(server ? { name: "link", server } : { name: "servers" }));
          },
        },
      ]}
    />
  );
}

const PROBLEM_ICONS: Partial<Record<RequestsProblem["kind"], IconName>> = {
  off: "ban",
  noAccess: "lock",
  tmdbDown: "server-off",
  tmdbKey: "circle-alert",
  signIn: "user-round",
};

/**
 * A page that can't show requests, saying why: off, not open to
 * this account (its requests still reachable when it has some), signing
 * in again, TMDB's key or TMDB not answering (Retry); anything else in the
 * app's own words, with Retry.
 */
export function RequestsMessage(p: { problem: RequestsProblem; title: string; onRetry: () => void }) {
  const app = useApp();
  const ui = useUi();
  const s = app.requests.value;
  const [signIn, setSignIn] = useState(false);
  const words = problemWords(p.problem, !!s?.manage);
  let action: { label: string; icon?: IconName; onSelect: () => void } | undefined;
  if (!words) {
    const e = p.problem.kind === "other" ? p.problem.error : null;
    return <Message title={failedTitle(e, p.title)} body={problem(e)} action={{ label: "Retry", icon: "refresh-cw", onSelect: p.onRetry }} />;
  }
  if (words.retry) action = { label: "Retry", icon: "refresh-cw", onSelect: p.onRetry };
  else if (p.problem.kind === "signIn") action = { label: "Sign in again", icon: "user-round", onSelect: () => setSignIn(true) };
  else if (p.problem.kind === "noAccess" && s && (s.own > 0 || s.manage)) action = { label: requestsTitle(s), icon: "inbox", onSelect: () => ui.nav.open({ name: "requests" }) };
  return (
    <>
      <Message icon={PROBLEM_ICONS[p.problem.kind]} title={words.title} body={words.body} action={action} />
      {signIn && <SignInAgainDialog onClose={() => setSignIn(false)} />}
    </>
  );
}

/** Why requests can't be used, from the summary alone (`null`: they may be, or it isn't known yet). */
export function summaryProblem(app: AppState): RequestsProblem | null {
  const s = app.requests.value;
  if (!s) return null;
  if (s.sign_in_again === "requests") return { kind: "signIn" };
  if (!s.enabled) return { kind: "off" };
  if (s.access === "off") return { kind: "noAccess" };
  return null;
}
