import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import type { MediaRequest, RequestList, RequestUser, TitleKind } from "../../api/types";
import { t } from "../../i18n";
import {
  actionDone,
  actionRows,
  askedLine,
  askRefusal,
  hasRequests,
  inRequestsTab,
  keepUnchanged,
  kindWord,
  mayAsk,
  moving,
  problemWords,
  requestNote,
  requestsEmpty,
  requestsProblem,
  requestsQuery,
  requestsTabs,
  requestsTitle,
  seasonsLabel,
  stageWords,
  tabCount,
  titleKey,
  type ActionId,
  type ActionRow,
  type RequestFilters,
  type RequestsTab,
} from "../../requests";
import { useUi, type Ui } from "../context";
import { Art } from "../kit/art";
import { Chip, PanelRow, Pill } from "../kit/buttons";
import { TextField } from "../kit/field";
import { F, G } from "../kit/focusable";
import { Icon, type IconName } from "../kit/icons";
import { memo } from "../kit/memo";
import { PanelTitle, SidePanel, useScrollToFocus } from "../kit/panel";
import type { Entry } from "../shell/navigator";
import { failedTitle, PageTitle, problem, useApp, useLoad } from "./common";
import { askFor, DownloadBar, hours24, RequestsMessage, SignInAgainDialog, StageBlock, StagePill, summaryProblem, usePoll } from "./request-kit";

/** Requests come 30 at a time, 30 more as focus nears the end, up to the 200 the server gives at once. */
export const REQUESTS_PAGE = 30;
const REQUESTS_MAX = 200;
/** The managers' title filter asks once typing pauses this long. */
const FILTER_DEBOUNCE_MS = 400;
/** The list starts under the tabs, or under the filters (or the line asking to sign in again). */
const LIST_TOP = 128;
const LIST_TOP_LOW = 178;
/** A card and the gap under it: the list scrolls by whole cards. */
const CARD_PITCH = 98;
/** The line after the newest 200, under the last card. */
const MORE_LINE = 4 + 17;
/** Answers kept for the tabs and filters seen, so going back to one draws it at once. */
const LISTS_KEPT = 8;

const ACTION_ICONS: Record<ActionId, IconName> = {
  withdraw: "x",
  unwatch: "bell-off",
  approve: "check",
  retry: "refresh-cw",
  decline: "ban",
  decline_reason: "pencil",
  handled: "circle-check",
  delete: "trash-2",
  ask_again: "rotate-ccw",
};

/**
 * Your requests, or for managers Requests: tabs by status, managers'
 * opening on Needs attention (pending, and failed ones the server won't
 * try again), with filters for movies or shows, a person and a title;
 * cards with the poster, the seasons, who asked and when, the reason or
 * note, the stage pill and the download's bar; OK opens what may be done
 * with one in a side panel.
 */
export function RequestsScreen(p: { entry: Entry }) {
  const app = useApp();
  const s = app.requests.value;
  // An account that may not ask still sees what it asked for before; one with nothing to see is told why.
  if (s && !hasRequests(s)) return <RequestsMessage problem={summaryProblem(app) ?? { kind: "off" }} title={t("requests.page.loadFailed")} onRetry={() => void app.loadRequests()} />;
  return <RequestsPage entry={p.entry} />;
}

type Panel = null | { kind: "request"; id: number } | { kind: "people" };

function RequestsPage(p: { entry: Entry }) {
  const app = useApp();
  const ui = useUi();
  const entry = p.entry;
  const s = app.requests.value;
  const manage = !!s?.manage;
  const tabs = requestsTabs(manage);
  const [, redraw] = useState(0);

  // The tab chosen here, else the first (Needs attention for managers, as soon as the summary says so).
  const chosen = entry.state.tab as RequestsTab | undefined;
  const tab: RequestsTab = chosen && tabs.some((x) => x.tab === chosen) ? chosen : tabs[0]!.tab;
  const kept = (entry.state.filters as RequestFilters | undefined) ?? { kind: null, userId: null, q: "" };
  const [filters, setFilterState] = useState<RequestFilters>(kept);
  const [person, setPerson] = useState<RequestUser | null>((entry.state.person as RequestUser | undefined) ?? null);
  const [text, setText] = useState(kept.q);
  const [limit, setLimit] = useState<number>((entry.state.limit as number | undefined) ?? REQUESTS_PAGE);
  const [panel, setPanel] = useState<Panel>(null);
  const [signIn, setSignIn] = useState(false);
  /** The card focus is on (or was last), which the list scrolls to. */
  const [focusAt, setFocusAt] = useState<number>((entry.state.index as number | undefined) ?? 0);
  const f: RequestFilters = manage ? filters : { kind: null, userId: null, q: "" };

  const setTab = (next: RequestsTab) => {
    if (next === tab) return;
    entry.state.tab = next;
    entry.state.limit = REQUESTS_PAGE;
    entry.state.index = 0;
    setLimit(REQUESTS_PAGE);
    setFocusAt(0);
    redraw((n) => n + 1);
  };
  const setFilters = (change: (f: RequestFilters) => RequestFilters) => {
    setFilterState((old) => {
      const next = change(old);
      entry.state.filters = next;
      return next;
    });
    entry.state.limit = REQUESTS_PAGE;
    entry.state.index = 0;
    setLimit(REQUESTS_PAGE);
    setFocusAt(0);
  };
  // The title filter asks once typing pauses.
  useEffect(() => {
    const q = text.trim();
    if (q === filters.q) return;
    const timer = setTimeout(() => setFilters((old) => ({ ...old, q })), FILTER_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [text]);

  // The list: asked again when requests change (their events), the newest answer for each tab and filter kept.
  const query = requestsQuery(tab, f, limit);
  const key = JSON.stringify(requestsQuery(tab, f, 0));
  const lists = (entry.state.lists as Record<string, RequestList> | undefined) ?? {};
  entry.state.lists = lists;
  const changes = app.requestChanges.value.n;
  const load = useLoad((c) => ui.api.requests(query, c).then((list) => ({ key, list })), [key, limit, changes], { live: false });
  const fresh = load.data && load.data.key === key ? load.data.list : null;
  // A new answer keeps the requests answered as before as they were, so that only the cards that changed draw again.
  const answered = useRef<RequestList | null>(null);
  if (fresh && answered.current !== fresh) {
    answered.current = fresh;
    const kept = keepUnchanged(lists[key], fresh);
    delete lists[key];
    lists[key] = kept;
    const keys = Object.keys(lists);
    for (const k of keys.slice(0, Math.max(0, keys.length - LISTS_KEPT))) delete lists[k];
  }
  const list = lists[key] ?? null;

  // What an action answered, until the list is asked again: the request as it is now, or `null` once it left the list.
  const [local, setLocal] = useState<{ base: RequestList | null; map: Record<number, MediaRequest | null> }>({ base: null, map: {} });
  const listNow = useRef(list);
  listNow.current = list;
  const override = (id: number, r: MediaRequest | null) =>
    setLocal((l) => {
      const base = listNow.current;
      return { base, map: { ...(l.base === base ? l.map : {}), [id]: r } };
    });
  const changed = local.base === list ? local.map : {};
  const shown: MediaRequest[] = [];
  for (const r of list?.requests ?? []) {
    const now = r.id in changed ? changed[r.id]! : r;
    if (now && inRequestsTab(now, tab)) shown.push(now);
  }

  // News since Your requests was last looked at keeps its dot while the page shows; the page tells the server it was seen.
  const news = useRef(new Set<number>());
  for (const r of list?.requests ?? []) if (r.unseen) news.current.add(r.id);
  const told = useRef(false);
  useEffect(() => {
    if (told.current || !fresh) return;
    told.current = true;
    if ((app.requests.value?.unseen ?? 0) > 0 || fresh.requests.some((r) => r.unseen)) {
      ui.api.markRequestsSeen().then(
        () => void app.loadRequests(true),
        () => undefined,
      );
    }
  }, [fresh]);

  // Downloads move without events: the list is asked every 10 s while one does; every 30 s while the events are down.
  usePoll(load.reload, moving(shown) ? 10_000 : app.eventsDown.value ? 30_000 : null);

  // ---- focus

  const page = useRef<HTMLDivElement>(null);
  const tabsBox = useRef<HTMLElement | null>(null);
  const filtersBox = useRef<HTMLElement | null>(null);
  const listGroup = useRef<HTMLElement | null>(null);
  const listBox = useRef<HTMLDivElement>(null);
  const index = useRef<number>((entry.state.index as number | undefined) ?? 0);
  const hadFocus = useRef(false);
  const lowTop = manage || s?.sign_in_again === "approvals";
  const listTop = lowTop ? LIST_TOP_LOW : LIST_TOP;
  const scrolled = useScrollToFocus(listBox, 540 - listTop);
  const moreLine = !!list && list.total > list.requests.length && limit >= REQUESTS_MAX;
  // By whole cards, the one above the focused in view, so none shows cut under the tabs; no further than the last needs.
  // The pointer's nudges stay as they are.
  const lastFits = Math.ceil((CARD_PITCH * (shown.length + (moreLine ? 1 : 0)) + 6 - (540 - listTop)) / CARD_PITCH);
  const shift = ui.focus.byPointer ? scrolled : CARD_PITCH * Math.max(0, Math.min(focusAt - 1, lastFits));
  // A card (or the line after the last) cut at the screen's foot: the list fades out there, as it goes on.
  const listEnd = 8 + CARD_PITCH * shown.length - 10 + (moreLine ? MORE_LINE : 0);
  const goesOn = listEnd - shift > 540 - listTop;

  const focusIn = (box: HTMLElement | null, selector = "[data-f]:not([data-fdisabled])") => {
    const el = box?.querySelector<HTMLElement>(selector) ?? null;
    return !!el && ui.focus.focus(el);
  };
  const toTab = () => focusIn(tabsBox.current, `[data-fid="tab/${tab}"]`) || focusIn(tabsBox.current);
  const toFilters = () => focusIn(filtersBox.current);
  const toList = () => {
    const group = listGroup.current;
    if (!group) return false;
    const el = ui.focus.rememberedIn(group) ?? group.querySelector<HTMLElement>(`[data-fid="req/${shown[Math.min(index.current, shown.length - 1)]?.id}"]`);
    return (!!el && ui.focus.focus(el)) || focusIn(group) || focusIn(page.current, ".requests-note [data-f]");
  };

  useEffect(
    () =>
      ui.focus.onChange((el) => {
        if (el && el.isConnected) hadFocus.current = !!page.current && page.current.contains(el);
      }),
    [],
  );
  // Focus that went with its card (withdrawn, approved off Needs attention, gone in a reload) or with a panel's opener:
  // the card now in its place, else the one before, else the tab.
  useLayoutEffect(() => {
    const at = ui.focus.current;
    if (!hadFocus.current || (at && at.isConnected) || panel || signIn) return;
    if (!page.current || ui.focus.root !== page.current.closest(".screen")) return;
    const r = shown[Math.min(index.current, shown.length - 1)];
    const el = r ? listGroup.current?.querySelector<HTMLElement>(`[data-fid="req/${r.id}"]`) : null;
    if (!(el && ui.focus.focus(el)) && !toTab()) ui.focus.move("down");
  });

  const onCard = (id: number) => {
    const i = shown.map((r) => r.id).indexOf(id);
    if (i < 0) return;
    index.current = i;
    entry.state.index = i;
    setFocusAt(i);
    // Five from the end: 30 more, while the server has more.
    if (!list || i < shown.length - 5 || list.total <= list.requests.length || limit > list.requests.length || limit >= REQUESTS_MAX) return;
    const next = Math.min(REQUESTS_MAX, list.requests.length + REQUESTS_PAGE);
    entry.state.limit = next;
    setLimit(next);
  };

  // ---- actions

  const busy = useRef(new Set<number>());
  const act = (r: MediaRequest, a: ActionId, reason: string | null = null) => {
    setPanel(null);
    if (busy.current.has(r.id)) return;
    busy.current.add(r.id);
    const done = () => {
      busy.current.delete(r.id);
      app.requestsChanged(titleKey(r));
      void app.loadRequests(true);
    };
    if (a === "ask_again") {
      // Asking again is asking: its toast says how it went.
      void askFor(app, ui, { kind: r.kind, tmdb_id: r.tmdb_id, seasons: r.seasons && r.seasons.length ? r.seasons : undefined }).then(() => busy.current.delete(r.id));
      return;
    }
    perform(ui, r, a, reason).then(
      (now) => {
        ui.toast(actionDone(a, r));
        // Withdrawn or no longer followed, it leaves this profile's list; a manager's list of everyone's is asked again.
        if (now) override(r.id, now);
        else if (a === "delete" || !manage) override(r.id, null);
        done();
      },
      (e: unknown) => {
        const why = requestsProblem(e);
        if (why.kind === "conflict" && why.request) override(r.id, why.request);
        if (why.kind === "missing") override(r.id, null);
        ui.toast(why.kind === "missing" ? t("requests.page.deletedMeanwhile") : askRefusal(why, r.kind, null), true);
        done();
      },
    );
  };

  // ---- drawing

  const people = list?.people ?? [];
  const personName = f.userId !== null ? (people.filter((u) => u.id === f.userId)[0]?.name ?? person?.name ?? null) : null;
  const panelRequest = panel && panel.kind === "request" ? (shown.filter((r) => r.id === panel.id)[0] ?? null) : null;
  // The request went while its panel was open: the panel goes too.
  useEffect(() => {
    if (panel && panel.kind === "request" && !panelRequest) setPanel(null);
  }, [!!panelRequest]);

  const why = !list && load.error ? requestsProblem(load.error) : null;
  if (why && problemWords(why, manage)) return <RequestsMessage problem={why} title={t("requests.page.loadFailed")} onRetry={load.reload} />;

  const h24 = hours24(app);
  const filtered = !!(f.kind || f.userId !== null || f.q);
  const findSome = mayAsk(s) && tab === "all" && !manage && !filtered;
  let body;
  if (!list) {
    body = load.error ? (
      <ListNote
        top={listTop}
        title={failedTitle(load.error, t("requests.page.loadFailed"))}
        body={problem(load.error)}
        action={{ label: t("common.actions.retry"), icon: "refresh-cw", onSelect: load.reload }}
      />
    ) : (
      <CardSkeletons top={listTop} />
    );
  } else if (!shown.length) {
    body = (
      <ListNote
        top={listTop}
        title={filtered ? t("requests.page.noMatch") : requestsEmpty(tab, manage)}
        action={findSome ? { label: t("requests.page.findSomething"), icon: "compass", onSelect: () => ui.nav.open({ name: "discover" }) } : undefined}
      />
    );
  } else {
    body = (
      <div class="requests-viewport" style={`top:${listTop}px`}>
        <div ref={listBox} class="scroll-y" style={`transform:translateY(${-shift}px)`}>
          <G
            kind="col"
            class="requests-list"
            elRef={(el) => (listGroup.current = el)}
            onEdge={(dir) => {
              if (dir === "up") return toFilters() || toTab();
              return dir === "right" || dir === "down";
            }}
          >
            {shown.map((r, i) => (
              <RequestCard
                key={r.id}
                request={r}
                manage={manage}
                hours24={h24}
                news={news.current.has(r.id)}
                fid={`req/${r.id}`}
                initial={i === 0}
                onFocus={() => onCard(r.id)}
                onSelect={() => setPanel({ kind: "request", id: r.id })}
              />
            ))}
          </G>
          {moreLine && <div class="requests-more t-caption c-muted">{t("requests.page.newest", { number: REQUESTS_MAX })}</div>}
        </div>
        <div class={`requests-fade${goesOn ? " shown" : ""}`} />
      </div>
    );
  }

  const counts = list?.counts ?? null;
  // Filtered, Needs attention counts what its list holds (the summary's number is everyone's): known while it shows.
  const count = (of: RequestsTab) => (of === "attention" && filtered ? (tab === "attention" && list ? list.total : null) : tabCount(of, counts, s));
  return (
    <div ref={page} class="requests-page">
      <PageTitle>{requestsTitle(s)}</PageTitle>
      <G
        kind="row"
        class="requests-tabs"
        elRef={(el) => (tabsBox.current = el)}
        onEnter={() => tabsBox.current?.querySelector<HTMLElement>(`[data-fid="tab/${tab}"]`) ?? null}
        onEdge={(dir) => {
          if (dir === "down") return toFilters() || toList();
          return dir === "right";
        }}
      >
        {tabs.map((x) => (
          <CountTab
            key={x.tab}
            fid={`tab/${x.tab}`}
            label={x.label}
            count={count(x.tab)}
            shown={x.tab === tab}
            initial={x.tab === tab && !!list && !shown.length && !findSome}
            onSelect={() => setTab(x.tab)}
          />
        ))}
      </G>
      {manage && (
        <G
          kind="row"
          class="requests-filters"
          elRef={(el) => (filtersBox.current = el)}
          onEdge={(dir) => {
            if (dir === "up") return toTab();
            if (dir === "down") return toList();
            return dir === "right";
          }}
        >
          <Chip
            fid="filter/kind"
            label={t(`requests.page.kinds.${f.kind ?? "all"}`)}
            icon="film"
            selected={!!f.kind}
            onSelect={() => setFilters((old) => ({ ...old, kind: nextKind(old.kind) }))}
          />
          <Chip
            fid="filter/person"
            label={personName ?? t("requests.page.everyone")}
            icon="users"
            selected={f.userId !== null}
            maxWidth={180}
            onSelect={() => setPanel({ kind: "people" })}
          />
          <TextField value={text} placeholder={t("requests.page.titleFilter")} icon="search" type="search" width={240} fid="filter/q" onInput={setText} />
        </G>
      )}
      {!manage && s?.sign_in_again === "approvals" && (
        <G
          kind="row"
          class="requests-filters"
          elRef={(el) => (filtersBox.current = el)}
          onEdge={(dir) => {
            if (dir === "up") return toTab();
            if (dir === "down") return toList();
            return dir === "right";
          }}
        >
          <Icon name="user-round" size={20} color="var(--muted)" />
          <div class="t-body c-muted one-line requests-signin">{t("requests.signInToDecide")}</div>
          <Pill label={t("requests.buttons.signIn")} fid="signin" onSelect={() => setSignIn(true)} />
        </G>
      )}
      {body}
      <div class="rail-fade" />
      {panelRequest && <RequestPanel request={panelRequest} manage={manage} onClose={() => setPanel(null)} onAction={act} />}
      {panel && panel.kind === "people" && (
        <PeoplePanel
          people={people}
          picked={f.userId}
          pickedName={personName}
          onClose={() => setPanel(null)}
          onPick={(u) => {
            setPanel(null);
            setPerson(u);
            entry.state.person = u ?? undefined;
            setFilters((old) => ({ ...old, userId: u ? u.id : null }));
          }}
        />
      )}
      {signIn && <SignInAgainDialog onClose={() => setSignIn(false)} />}
    </div>
  );
}

/** Movies and shows, then movies, then shows. */
function nextKind(kind: TitleKind | null): TitleKind | null {
  return kind === null ? "movie" : kind === "movie" ? "show" : null;
}

/** What an action did to the request: as it is now, or `null` when it's gone for this profile. */
async function perform(ui: Ui, r: MediaRequest, a: ActionId, reason: string | null): Promise<MediaRequest | null> {
  switch (a) {
    case "approve":
    case "retry":
      return ui.api.approveRequest(r.id);
    case "decline":
      return ui.api.declineRequest(r.id, null);
    case "decline_reason":
      return ui.api.declineRequest(r.id, reason && reason.trim() ? reason.trim() : null);
    case "handled":
      return ui.api.markRequestHandled(r.id);
    case "withdraw":
    case "delete":
      await ui.api.deleteRequest(r.id);
      return null;
    case "unwatch":
      await ui.api.unwatchRequest(r.id);
      return null;
    default:
      return null;
  }
}

/** A tab with its number after the name (none while it isn't known). */
function CountTab(p: { fid: string; label: string; count: number | null; shown: boolean; initial?: boolean; onSelect: () => void }) {
  const n = p.count;
  return (
    <F class={`tab t-label${p.shown ? " shown" : ""}`} fid={p.fid} initial={p.initial} label={n !== null ? `${p.label}, ${n}` : p.label} onSelect={p.onSelect}>
      <span class="one-line">{p.label}</span>
      {n !== null && n > 0 && <span class="tab-count">{n > 999 ? "999+" : String(n)}</span>}
    </F>
  );
}

/** A request as a card: poster, title and year, what was asked for, who asked and when, the reason or note, the stage. */
const RequestCard = memo(RequestCardView);

function RequestCardView(p: {
  request: MediaRequest;
  manage: boolean;
  hours24: boolean;
  /** News since Your requests was last looked at. */
  news: boolean;
  fid: string;
  initial?: boolean;
  onFocus: () => void;
  onSelect: () => void;
}) {
  const r = p.request;
  const stage = stageWords(r, { manage: p.manage, hours24: p.hours24 });
  const what = requestWhat(r);
  const asked = askedLine(r, p.manage);
  const note = [stage.note, p.manage ? requestNote(r) : null].filter(Boolean).join(" ");
  return (
    <F
      class="request-card"
      fid={p.fid}
      initial={p.initial}
      label={[r.title, r.year ? String(r.year) : "", what, stage.text, note, asked, p.news ? t("requests.page.new") : ""].filter(Boolean).join(", ")}
      onFocus={p.onFocus}
      onSelect={p.onSelect}
    >
      <div class="request-poster">
        <Art candidates={r.poster_url ? [r.poster_url] : []} width={48} title={r.title} class="tile-clip" />
      </div>
      {p.news && <i class="request-news" />}
      <div class="request-text">
        <div class="t-title one-line">
          {r.title}
          {r.year ? <span class="request-year">{` · ${r.year}`}</span> : null}
        </div>
        <div class="t-caption one-line sub">{`${what} · ${asked}`}</div>
        {note && <div class="t-caption one-line note">{note}</div>}
      </div>
      <div class="request-stage">
        <StageBlock words={stage} />
      </div>
      <div class="card-ring" />
    </F>
  );
}

/** "Movie", or a show's seasons ("Seasons 1–2"). */
function requestWhat(r: MediaRequest): string {
  return (r.kind === "show" && seasonsLabel(r.seasons)) || kindWord(r.kind);
}

/** Where the cards go while the first answer comes. */
function CardSkeletons(p: { top: number }) {
  return (
    <div role="status" aria-label={t("common.states.loading")}>
      {[0, 1, 2, 3].map((i) => (
        <div key={i} class="skeleton" style={`position:absolute;left:96px;top:${p.top + 8 + i * 98}px;width:816px;height:88px;border-radius:6px`} />
      ))}
    </div>
  );
}

/** What the list says instead of cards: none, or why none came, with a button where one helps. */
function ListNote(p: { top: number; title: string; body?: string; action?: { label: string; icon?: IconName; onSelect: () => void } }) {
  return (
    <div class="requests-note" style={`top:${p.top + 24}px`} role="status">
      <div class="t-section c-fg">{p.title}</div>
      {p.body && (
        <div class="t-body c-muted" style="margin-top:8px;max-width:520px">
          {p.body}
        </div>
      )}
      {p.action && (
        <G kind="row" style="margin-top:20px">
          <Pill label={p.action.label} icon={p.action.icon} fid="note" initial onSelect={p.action.onSelect} />
        </G>
      )}
    </div>
  );
}

type Step = { kind: "actions" } | { kind: "confirm"; row: ActionRow } | { kind: "reason" };

/**
 * What may be done with a request, in a side panel: Open (when the library
 * has it for this profile), its title's page, and the server's actions in
 * its order. What can't be undone asks first, in the panel; Decline with a
 * reason… asks for the words with the TV's keyboard.
 */
export function RequestPanel(p: { request: MediaRequest; manage: boolean; onClose: () => void; onAction: (r: MediaRequest, a: ActionId, reason?: string | null) => void }) {
  const app = useApp();
  const ui = useUi();
  const r = p.request;
  const [step, setStep] = useState<Step>({ kind: "actions" });
  const [reason, setReason] = useState("");
  const last = useRef<string | null>(null);

  // Back from a question goes back to the actions, not out of the panel.
  useLayoutEffect(() => {
    if (step.kind === "actions") return;
    return ui.input.push((key, e) => {
      if (key !== "back") return false;
      if (e.type === "down" && !e.repeat) setStep({ kind: "actions" });
      return true;
    });
  }, [step.kind]);
  // The row focused went (the request changed while the panel was open): the panel's first.
  useLayoutEffect(() => {
    const at = ui.focus.current;
    if (at && at.isConnected) return;
    const el = document.querySelector<HTMLElement>(".side-panel [data-f]:not([data-fdisabled])");
    if (el) ui.focus.focus(el);
  });

  const back = () => setStep({ kind: "actions" });
  const rows = actionRows(r);
  let content;
  if (step.kind === "confirm") {
    const c = step.row.confirm!;
    // Cancel first, as the app's other questions: OK pressed twice undoes nothing.
    content = (
      <>
        <PanelTitle>{c.title}</PanelTitle>
        {c.body && <div class="panel-text t-body c-muted">{c.body}</div>}
        <PanelRow fid="cancel" label={t("common.actions.cancel")} icon="arrow-left" initial onSelect={back} />
        <PanelRow fid="confirm" label={c.button} icon={ACTION_ICONS[step.row.id]} danger onSelect={() => p.onAction(r, step.row.id)} />
      </>
    );
  } else if (step.kind === "reason") {
    content = (
      <>
        <PanelTitle>{t("requests.decline.title", { title: r.title })}</PanelTitle>
        {!r.mine && (
          <div class="panel-text t-body c-muted">
            {r.requested_by ? t("requests.decline.nameSees", { name: r.requested_by.name }) : t("requests.decline.someoneSees")}
          </div>
        )}
        <div class="panel-field">
          <TextField
            value={reason}
            placeholder={t("requests.decline.reason")}
            fid="reason"
            width={288}
            initial
            typeNow
            onInput={setReason}
            // The keyboard's Done: the words are in, and Decline is next.
            onSubmit={() => {
              const el = document.querySelector<HTMLElement>('.side-panel [data-fid="decline"]');
              if (el) ui.focus.focus(el);
            }}
          />
        </div>
        <PanelRow fid="decline" label={t("requests.actions.decline")} icon="ban" onSelect={() => p.onAction(r, "decline_reason", reason)} />
        <PanelRow fid="cancel" label={t("common.actions.cancel")} icon="arrow-left" onSelect={back} />
      </>
    );
  } else {
    const stage = stageWords(r, { manage: p.manage, hours24: hours24(app) });
    const facts = [requestWhat(r), askedLine(r, p.manage)].join(" · ");
    const first = r.item_id !== null ? "open" : rows.length ? rows[0]!.id : "details";
    const initial = (id: string) => (last.current ? last.current === id : first === id);
    content = (
      <>
        <PanelTitle>{r.year ? `${r.title} (${r.year})` : r.title}</PanelTitle>
        <div class="panel-request">
          <StagePill words={stage} small />
          {stage.progress !== null && <DownloadBar progress={stage.progress} width={288} />}
          <div class="t-caption c-muted lines-2" style="margin-top:8px">
            {facts}
          </div>
          {stage.note && <div class="t-caption c-muted lines-3">{stage.note}</div>}
          {p.manage && requestNote(r) && <div class="t-caption c-muted lines-2">{requestNote(r)}</div>}
        </div>
        {r.item_id !== null && (
          <PanelRow
            fid="open"
            label={t("requests.buttons.open")}
            icon="play"
            initial={initial("open")}
            onSelect={() => {
              const id = r.item_id!;
              p.onClose();
              ui.nav.open({ name: "item", id });
            }}
          />
        )}
        {rows.map((row) => (
          <PanelRow
            key={row.id}
            fid={`action/${row.id}`}
            label={row.label}
            icon={ACTION_ICONS[row.id]}
            danger={row.danger}
            initial={initial(row.id)}
            onSelect={() => {
              last.current = row.id;
              if (row.id === "decline_reason") setStep({ kind: "reason" });
              else if (row.confirm) setStep({ kind: "confirm", row });
              else p.onAction(r, row.id);
            }}
          />
        ))}
        <PanelRow
          fid="details"
          label={t("requests.buttons.details")}
          icon="info"
          initial={initial("details")}
          onSelect={() => {
            p.onClose();
            ui.nav.open({ name: "title", kind: r.kind, tmdbId: r.tmdb_id });
          }}
        />
      </>
    );
  }
  return (
    <SidePanel onClose={p.onClose} label={r.title}>
      {content}
    </SidePanel>
  );
}

/** Whose requests the managers' list shows: everyone's, or one person's. */
function PeoplePanel(p: { people: RequestUser[]; picked: number | null; pickedName: string | null; onClose: () => void; onPick: (u: RequestUser | null) => void }) {
  // The person picked stays offered when the other filters leave them out.
  let people = p.people;
  if (p.picked !== null && p.pickedName && !people.some((u) => u.id === p.picked)) {
    const picked: RequestUser = { id: p.picked, name: p.pickedName, avatar_url: null, avatar_color: null };
    people = [picked].concat(people);
  }
  return (
    <SidePanel onClose={p.onClose} label={t("requests.page.askedBy")}>
      <PanelTitle>{t("requests.page.askedBy")}</PanelTitle>
      <PanelRow fid="person/all" label={t("requests.page.everyone")} icon="users" checked={p.picked === null} initial={p.picked === null} onSelect={() => p.onPick(null)} />
      {people.map((u) => (
        <PanelRow key={u.id} fid={`person/${u.id}`} label={u.name} icon="user-round" checked={u.id === p.picked} initial={u.id === p.picked} onSelect={() => p.onPick(u)} />
      ))}
    </SidePanel>
  );
}
