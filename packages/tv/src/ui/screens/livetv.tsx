import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "preact/hooks";
import type { LiveChannel, LiveProgram } from "../../api/types";
import {
  airing,
  browseQuery,
  categoryKey,
  categoryName,
  channelByNumber,
  channelName,
  episodeText,
  formatGuideDay,
  formatSpan,
  formatTime,
  groupsMatching,
  nowAndNext,
  parseCategory,
  programProgress,
  timeLeft,
  type LiveCategory,
} from "../../live/data";
import { ChannelPages } from "../../live/pages";
import { t } from "../../i18n";
import { useUi } from "../context";
import { Chip, PanelRow, Tab } from "../kit/buttons";
import { F, G } from "../kit/focusable";
import { Icon } from "../kit/icons";
import { SidePanel } from "../kit/panel";
import type { Entry } from "../shell/navigator";
import { ColumnMessage, failedTitle, PageBadge, problem, useApp, useLoad } from "./common";
import { CategoryPanel } from "./live-categories";
import { LiveGuide } from "./live-guide";
import { ChannelLogo, NumberBox, ProgramMarks, ProgressLine, useNow, useNumberEntry } from "./live-kit";

/** The view and the category last shown, kept on the TV (the web keeps them in the browser). */
const VIEW_KEY = "pley.live.view";
const CATEGORY_KEY = "pley.live.category";

type View = "guide" | "channels";
type Panel = null | { kind: "categories" } | { kind: "details"; channel: LiveChannel; program: LiveProgram | null; opener: string | null };

/** A channel to bring into view: its place in the category, asked anew each time. */
export interface Jump {
  index: number;
  n: number;
}

/**
 * Live TV (the web's LiveTvPage): a category of
 * channels at a time, in the guide (channels down, time across) or as
 * cards with what's on now and next. Left from the channels opens the
 * categories: Favourites, Recently watched, All channels, then the
 * provider's groups by region. The channels come a page at a time, and
 * the guide only for the rows in view, so a lineup of tens of thousands
 * opens at once. A channel's number typed goes to it; coming back from the
 * player lands on the channel watched. Sources and the lineup are set up
 * in Pley on the web, as the TV apps' other administration is.
 */
export function LiveTvScreen(p: { entry: Entry }) {
  const app = useApp();
  const ui = useUi();
  const summary = app.live.value;
  const changes = app.liveChanges.value;
  const groups = useLoad((c) => ui.api.liveGroups(c), [changes]);
  const [category, setCategoryState] = useState<LiveCategory>(() => parseCategory(app.store.get(CATEGORY_KEY)));
  // A group that went (hidden, or the source read again without it): every channel.
  const gone = category.kind === "group" && !!groups.data && !groups.data.groups.some((g) => g.name === category.group);
  const shown: LiveCategory = gone ? { kind: "all" } : category;
  const key = categoryKey(shown);
  const pages = useMemo(() => new ChannelPages(ui.api, shown), [key, ui.api]);
  const total = pages.total.value;
  pages.version.value;
  const now = useNow();
  const minute = Math.floor(now / 60);

  // What's on moves on, and the server's changes come: the pages in view are asked for again.
  const firstChanges = useRef(changes);
  useEffect(() => {
    if (changes !== firstChanges.current) pages.stale();
  }, [changes]);
  useEffect(() => void app.loadLive(), []);

  const [view, setViewState] = useState<View>(() => (app.store.get(VIEW_KEY) === "channels" ? "channels" : "guide"));
  const [jump, setJump] = useState<Jump | null>(null);
  const [panel, setPanel] = useState<Panel>(null);
  const [missing, setMissing] = useState<string | null>(null);
  const tabsRef = useRef<HTMLElement | null>(null);
  /** A channel to go to once the category it is in shows. */
  const jumpTo = useRef<number | null>(null);

  const setView = (v: View) => {
    app.store.set(VIEW_KEY, v);
    // A channel brought into view before was for the view shown then.
    setJump(null);
    setViewState(v);
  };
  const setCategory = (c: LiveCategory) => {
    // A search shows until Back; the category kept is the one it was opened over.
    if (c.kind !== "search") app.store.set(CATEGORY_KEY, categoryKey(c));
    p.entry.state.guideRow = undefined;
    p.entry.state.cardRow = undefined;
    setJump(null);
    setCategoryState(c);
  };

  // Back from the player: the channel watched, in the category it was played in.
  useEffect(() => {
    const last = app.liveLastWatched;
    if (!last) return;
    app.liveLastWatched = null;
    if (last.category !== key) setCategory(parseCategory(last.category));
    jumpTo.current = last.id;
  }, []);
  // A channel to go to: its place in the category shown.
  useEffect(() => {
    const id = jumpTo.current;
    if (id === null) return;
    let live = true;
    pages.indexOf(id).then(
      (index) => {
        if (!live) return;
        jumpTo.current = null;
        if (index !== null) setJump({ index, n: Date.now() });
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [pages]);

  const searching = shown.kind === "search";
  useEffect(() => {
    if (!searching) return;
    return ui.input.push((key, e) => {
      if (key !== "back" || panel) return false;
      if (e.type === "down" && !e.repeat) setCategory(parseCategory(app.store.get(CATEGORY_KEY)));
      return true;
    });
  }, [searching, !!panel]);

  const watch = (c: LiveChannel) => {
    app.keepLiveChannel(c);
    ui.nav.open({ name: "livePlayer", channelId: c.id, category: key, pressedAt: Date.now() });
  };
  const details = (c: LiveChannel, program: LiveProgram | null) =>
    setPanel({ kind: "details", channel: c, program, opener: ui.focus.current?.getAttribute("data-fid") ?? null });
  const focusFid = (fid: string | null) =>
    setTimeout(() => {
      const el = fid ? document.querySelector<HTMLElement>(`.live-page [data-fid="${fid}"]`) : null;
      if (el) ui.focus.focus(el);
    }, 0);
  const closePanel = () => {
    const opener = panel?.kind === "details" ? panel.opener : null;
    setPanel(null);
    focusFid(opener);
  };
  const toggleFavorite = (c: LiveChannel) => {
    const on = !c.favorite;
    pages.patch(c.id, { favorite: on });
    if (panel?.kind === "details") setPanel({ ...panel, channel: { ...panel.channel, favorite: on } });
    ui.api.setLiveFavorite(c.id, on).then(
      () => {
        ui.toast(on ? t("livetv.favourites.added", { name: c.name }) : t("livetv.favourites.removed", { name: c.name }));
        groups.reload();
      },
      () => {
        pages.patch(c.id, { favorite: !on });
        ui.toast(on ? t("livetv.favourites.addFailed") : t("livetv.favourites.removeFailed"), true);
      },
    );
  };

  // A number typed: that channel in this category, else among every channel.
  const digits = useNumberEntry((d) => {
    const find = (c: LiveCategory) =>
      ui.api.liveBrowse({ ...browseQuery(c.kind === "search" ? { kind: "all" } : c), q: d, limit: 50 }).then((page) => channelByNumber(page.channels, d));
    find(shown)
      .then((found) => (found ? { found, here: true } : find({ kind: "all" }).then((f) => ({ found: f, here: false }))))
      .then(
        ({ found, here }) => {
          if (!found) {
            setMissing(d);
            return;
          }
          jumpTo.current = found.id;
          if (!here) setCategory({ kind: "all" });
          else
            pages.indexOf(found.id).then(
              (index) => {
                jumpTo.current = null;
                if (index !== null) setJump({ index, n: Date.now() });
              },
              () => undefined,
            );
        },
        () => setMissing(d),
      );
  }, !panel);
  useEffect(() => {
    if (!missing) return;
    const t = setTimeout(() => setMissing(null), 2_000);
    return () => clearTimeout(t);
  }, [missing]);

  if (summary && !summary.allowed) {
    return (
      <div class="live-page">
        <ColumnMessage title={t("livetv.page.notOpen")} body={t("livetv.page.notOpenHint")} />
      </div>
    );
  }
  const error = pages.error.value;
  if (total === null && error) {
    return (
      <div class="live-page">
        <ColumnMessage
          title={failedTitle(error, t("livetv.page.didntLoad"))}
          body={problem(error)}
          action={{ label: t("common.actions.retry"), icon: "refresh-cw", onSelect: () => pages.want(0, 0) }}
        />
      </div>
    );
  }
  if (total === 0 && shown.kind === "all") {
    const admin = !!app.me.value?.is_admin;
    return (
      <div class="live-page">
        <ColumnMessage
          title={admin ? t("livetv.page.setUpTitle") : t("livetv.page.noChannelsTitle")}
          body={admin ? t("livetv.page.setUpHint") : t("livetv.page.noChannelsHint")}
        />
      </div>
    );
  }

  const openCategories = () => setPanel({ kind: "categories" });
  // A search's groups whose names match, to go to.
  const found = shown.kind === "search" ? groupsMatching(groups.data?.groups ?? [], shown.q, 2) : [];
  const focusTabs = () => {
    const el = tabsRef.current?.querySelector<HTMLElement>(`[data-fid="tab/${view}"]`);
    return !!el && ui.focus.focus(el);
  };
  const name = categoryName(shown, groups.data?.groups);
  const hours24 = app.platform.clock24();
  return (
    <div class="live-page">
      <div class="library-header">
        <div class="t-page c-fg one-line" style="flex:1;min-width:0">
          {t("livetv.title")}
        </div>
        <G kind="row" class="library-tabs" elRef={(el) => (tabsRef.current = el)} onEnter={() => tabsRef.current?.querySelector<HTMLElement>(`[data-fid="tab/${view}"]`) ?? null}>
          <Tab fid="tab/guide" label={t("livetv.guideTab")} shown={view === "guide"} onSelect={() => setView("guide")} />
          <Tab fid="tab/channels" label={t("livetv.channels")} shown={view === "channels"} onSelect={() => setView("channels")} />
        </G>
      </div>
      <G kind="row" class="library-chips">
        <Chip fid="chip/category" label={name} icon={categoryIcon(shown)} maxWidth={360} onSelect={openCategories} />
        {found.map((g) => (
          <Chip key={g.name} fid={`chip/group:${g.name}`} label={g.region ? `${g.region} · ${g.label}` : g.label} maxWidth={170} onSelect={() => setCategory({ kind: "group", group: g.name })} />
        ))}
        <div class="t-caption c-muted chips-count">{total === null ? "" : t("livetv.page.channelCount", { count: total })}</div>
      </G>
      {total === 0 ? (
        <EmptyCategory shown={shown} onSearch={openCategories} onAll={() => setCategory({ kind: "all" })} />
      ) : view === "guide" ? (
        <LiveGuide
          key={key}
          entry={p.entry}
          pages={pages}
          category={shown}
          now={now}
          minute={minute}
          jump={jump}
          hours24={hours24}
          onWatch={watch}
          onDetails={details}
          onCategories={openCategories}
          focusAbove={focusTabs}
          version={changes}
        />
      ) : (
        <ChannelCards key={key} entry={p.entry} pages={pages} category={shown} now={now} minute={minute} hours24={hours24} jump={jump} onWatch={watch} onDetails={details} onCategories={openCategories} />
      )}
      <NumberBox digits={digits} missing={missing} />
      {panel?.kind === "categories" && (
        <CategoryPanel
          groups={groups.data}
          selected={shown}
          left={72}
          onClose={() => {
            setPanel(null);
            focusFid(null);
          }}
          onSelect={(c) => {
            setPanel(null);
            setCategory(c);
          }}
        />
      )}
      {panel?.kind === "details" && (
        <ProgramPanel
          channel={panel.channel}
          category={shown}
          program={panel.program}
          now={now}
          hours24={hours24}
          onClose={closePanel}
          onWatch={() => {
            setPanel(null);
            watch(panel.channel);
          }}
          onFavorite={() => toggleFavorite(panel.channel)}
        />
      )}
    </div>
  );
}

function categoryIcon(c: LiveCategory) {
  return c.kind === "favorites" ? "star" : c.kind === "recent" ? "history" : c.kind === "all" ? "layers" : c.kind === "search" ? "search" : "list";
}

/** A category with no channels: why, and where to go instead (another search, or every channel). */
function EmptyCategory(p: { shown: LiveCategory; onSearch: () => void; onAll: () => void }) {
  const c = p.shown;
  const title =
    c.kind === "favorites" ? t("livetv.page.noFavourites") : c.kind === "recent" ? t("livetv.page.nothingWatched") : c.kind === "search" ? t("livetv.page.noMatch") : t("livetv.page.empty");
  const body =
    c.kind === "favorites"
      ? t("livetv.page.noFavouritesHint")
      : c.kind === "recent"
        ? t("livetv.page.nothingWatchedHint")
        : c.kind === "search"
          ? t("livetv.page.noMatchHint", { search: c.q })
          : t("livetv.page.emptyHint");
  return (
    <ColumnMessage
      top={146}
      title={title}
      body={body}
      action={
        c.kind === "search"
          ? { label: t("livetv.page.searchAgain"), icon: "search", onSelect: p.onSearch }
          : { label: t("livetv.categories.all"), icon: "layers", onSelect: p.onAll }
      }
    />
  );
}

// ---- a programme's details

/** A programme (or a channel without one) in a side panel: what the guide says of it, Watch, and the channel's favourite. */
function ProgramPanel(p: {
  channel: LiveChannel;
  category: LiveCategory;
  program: LiveProgram | null;
  now: number;
  hours24: boolean;
  onClose: () => void;
  onWatch: () => void;
  onFavorite: () => void;
}) {
  const c = p.channel;
  const prog = p.program;
  const on = prog ? airing(prog, p.now) : true;
  const ended = !!prog && prog.stop <= p.now;
  const episode = prog ? episodeText(prog) : null;
  const sub = prog ? [episode, prog.subtitle].filter(Boolean).join(" · ") : "";
  const name = channelName(c, p.category);
  return (
    <SidePanel onClose={p.onClose} label={prog ? prog.title : name} wide>
      <div class="live-panel-channel">
        <ChannelLogo channel={c} width={72} height={40} />
        <div class="t-label c-muted one-line" style="margin-left:12px;min-width:0">
          {c.number ? `${c.number}  ${name}` : name}
        </div>
      </div>
      {prog ? (
        <div class="live-panel-text">
          <div class="t-title c-fg lines-2">{prog.title}</div>
          {sub && <div class="t-label c-muted one-line" style="margin-top:4px">{sub}</div>}
          <div class="live-panel-facts">
            <span class="t-caption c-muted nowrap">{`${formatGuideDay(prog.start, p.now)} · ${formatSpan(prog, p.hours24)}`}</span>
            <ProgramMarks program={prog} />
            {prog.rating && <PageBadge text={prog.rating} />}
          </div>
          {on && (
            <div class="live-panel-progress">
              <ProgressLine fraction={programProgress(prog, p.now)} />
              <span class="t-caption c-muted nowrap" style="margin-left:10px">
                {timeLeft(prog, p.now)}
              </span>
            </div>
          )}
          <div class={`t-body ${prog.description ? "c-fg" : "c-subtle"} lines-6`} style="margin-top:10px">
            {prog.description || t("livetv.program.noDescription")}
          </div>
          {prog.categories.length > 0 && <div class="t-caption c-subtle one-line" style="margin-top:8px">{prog.categories.join(" · ")}</div>}
          {ended && (
            <div class="t-caption c-subtle" style="margin-top:8px">
              {t("livetv.program.ended")}
            </div>
          )}
        </div>
      ) : (
        <div class="live-panel-text">
          <div class="t-title c-fg">{name}</div>
          <div class="t-body c-subtle" style="margin-top:6px">
            {c.has_guide ? t("livetv.program.nothingThen") : t("livetv.program.nothingForChannel")}
          </div>
        </div>
      )}
      <PanelRow icon="play" label={on ? t("livetv.program.watchNow") : t("livetv.program.watchChannel")} initial onSelect={p.onWatch} />
      <PanelRow icon={c.favorite ? "star" : "star-outline"} label={c.favorite ? t("livetv.favourites.remove") : t("livetv.favourites.add")} onSelect={p.onFavorite} />
    </SidePanel>
  );
}

// ---- the channels as cards

/** Two cards a row, 400 wide (the stylesheet's) and 92 high, 16 apart across and down. */
const CARD_H = 92;
const CARD_GAP = 16;
const CARD_PITCH = CARD_H + CARD_GAP;
/** The cards' box, under the chips; the focused row sits a row from its top once it scrolls. */
const CARDS_TOP = 140;
const ROWS_ABOVE = 1;
/** Rows drawn above and below what shows. */
const CARD_ROWS_DRAWN = 3;

/**
 * The channels as cards, two a row (the web's ChannelGrid on a TV): the
 * logo, the channel, what's on with how far it is, and what's next. Only
 * the rows near the screen are drawn, and their pages asked for.
 */
function ChannelCards(p: {
  entry: Entry;
  pages: ChannelPages;
  category: LiveCategory;
  now: number;
  minute: number;
  hours24: boolean;
  jump: Jump | null;
  onWatch: (c: LiveChannel) => void;
  onDetails: (c: LiveChannel, program: LiveProgram | null) => void;
  onCategories: () => void;
}) {
  const ui = useUi();
  const total = p.pages.total.value ?? 0;
  p.pages.version.value;
  const rowsCount = Math.ceil(total / 2);
  const [row, setRowState] = useState(() => Math.min(Math.max(0, rowsCount - 1), (p.entry.state.cardRow as number | undefined) ?? 0));
  const setRow = (r: number) => {
    p.entry.state.cardRow = r;
    setRowState(r);
  };
  const visible = Math.floor((540 - CARDS_TOP) / CARD_PITCH);
  const shift = Math.max(0, Math.min(Math.max(0, rowsCount - visible) * CARD_PITCH, (row - ROWS_ABOVE) * CARD_PITCH));
  const first = Math.max(0, Math.floor(shift / CARD_PITCH) - CARD_ROWS_DRAWN);
  const last = Math.min(rowsCount, Math.ceil(shift / CARD_PITCH) + visible + CARD_ROWS_DRAWN);
  useEffect(() => p.pages.want(first * 2, last * 2), [first, last, p.pages, p.minute]);

  // A channel brought into view: its row, and focus on its card.
  const pendingFocus = useRef<number | null>(null);
  useEffect(() => {
    if (!p.jump) return;
    pendingFocus.current = p.jump.index;
    setRow(Math.floor(p.jump.index / 2));
  }, [p.jump?.n]);
  useLayoutEffect(() => {
    const want = pendingFocus.current;
    const current = ui.focus.current;
    const lost = !!current && !current.isConnected && current.classList.contains("live-card");
    if (want === null && !lost) return;
    const index = want ?? row * 2;
    const el = document.querySelector<HTMLElement>(`.live-cards [data-index="${index}"]`);
    if (!el) return;
    pendingFocus.current = null;
    ui.focus.focus(el);
  });

  const rows = [];
  for (let r = first; r < last; r++) {
    const cards = [];
    for (let i = r * 2; i < Math.min(total, r * 2 + 2); i++) {
      const c = p.pages.at(i);
      const onFocus = () => {
        if (r !== row) setRow(r);
      };
      cards.push(
        c ? (
          <ChannelCard
            key={i}
            channel={c}
            index={i}
            category={p.category}
            initial={i === row * 2}
            now={p.now}
            hours24={p.hours24}
            onFocus={onFocus}
            onWatch={() => p.onWatch(c)}
            onDetails={(prog) => p.onDetails(c, prog)}
          />
        ) : (
          <F key={i} class="live-card loading" fid={`card/at${i}`} initial={i === row * 2} label={t("common.states.loading")} onFocus={onFocus}>
            <span data-index={i} style="display:none" />
            <div class="skeleton live-card-logo" style="width:120px;height:68px" />
          </F>
        ),
      );
    }
    rows.push(
      <G
        key={r}
        kind="row"
        class="live-card-row"
        style={`top:${r * CARD_PITCH}px`}
        onEdge={(dir) => {
          if (dir !== "left") return false;
          p.onCategories();
          return true;
        }}
      >
        {cards}
      </G>,
    );
  }
  return (
    <div class="live-cards" style={`top:${CARDS_TOP}px`}>
      <G kind="col" class="live-cards-rows" style={`height:${rowsCount * CARD_PITCH}px;transform:translateY(${-shift}px)`}>
        {rows}
      </G>
    </div>
  );
}

/** One channel as a card: the logo, its number and name, what's on with its progress, and what's next. */
function ChannelCard(p: {
  channel: LiveChannel;
  index: number;
  category: LiveCategory;
  initial: boolean;
  now: number;
  hours24: boolean;
  onFocus: () => void;
  onWatch: () => void;
  onDetails: (program: LiveProgram | null) => void;
}) {
  const c = p.channel;
  const name = channelName(c, p.category);
  const { on, next } = nowAndNext(c, p.now);
  const numbered = `${c.number ? `${c.number} ` : ""}${name}`;
  const label = on ? t("livetv.cards.now", { name: numbered, title: on.title }) : numbered;
  return (
    <F class="live-card" fid={`card/${c.id}`} initial={p.initial} label={label} onFocus={p.onFocus} onSelect={p.onWatch} onLongSelect={() => p.onDetails(on)} onMenu={() => p.onDetails(on)}>
      <span data-index={p.index} style="display:none" />
      <ChannelLogo channel={c} width={120} height={68} class="live-card-logo" showName />
      <div class="live-card-text">
        <div class="live-card-name">
          {c.number && <span class="t-label live-number">{c.number}</span>}
          <span class="t-label one-line live-card-title">{name}</span>
          {c.favorite && <Icon name="star" size={14} class="live-star" />}
        </div>
        {on ? (
          <>
            <div class="t-body one-line live-card-now">{on.title}</div>
            <div class="live-card-progress">
              <ProgressLine fraction={programProgress(on, p.now)} />
              <span class="t-caption live-card-sub nowrap" style="margin-left:8px">
                {formatTime(on.stop, p.hours24)}
              </span>
            </div>
          </>
        ) : (
          <div class="t-body one-line live-card-sub">{c.has_guide ? t("livetv.nothingNow") : t("livetv.noGuideHere")}</div>
        )}
        {next && <div class="t-caption one-line live-card-sub">{`${formatTime(next.start, p.hours24)} · ${next.title}`}</div>}
      </div>
    </F>
  );
}
