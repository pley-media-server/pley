import { useEffect } from "preact/hooks";
import type { ItemDetail, ItemSummary } from "../../api/types";
import { episodeLabel, isInProgress, cardSubtitle } from "../../format/format";
import { t } from "../../i18n";
import type { AppState } from "../../state";
import { useUi, type Ui } from "../context";
import { artCandidates } from "../kit/art";
import { Backdrop } from "../kit/backdrop";
import { PanelRow } from "../kit/buttons";
import { G } from "../kit/focusable";
import type { IconName } from "../kit/icons";
import { PanelTitle, SidePanel } from "../kit/panel";
import { failedTitle, Message, problem, useApp, useLoad } from "./common";
import { hideFromContinueWatching, inMyList, pageOf, playItem, setInMyList, setWatched } from "./item-actions";

export interface ItemAction {
  id: string;
  label: string;
  icon?: IconName;
  /** Runs it; the screen or panel has gone already. */
  run: () => void;
}

const MUSIC = ["artist", "album", "track"];

/**
 * An item's actions, in the Android app's order (ItemActions): play or
 * resume, watched, Continue Watching, My List, a playlist, its show and
 * season. The item page's panel leaves out My List and Continue Watching.
 */
export function itemActions(
  app: AppState,
  ui: Ui,
  item: ItemSummary,
  opts: { continueWatching?: boolean; inPanel?: boolean; detail?: ItemDetail | null } = {},
): ItemAction[] {
  const out: ItemAction[] = [];
  if (MUSIC.indexOf(item.kind) >= 0) {
    out.push({ id: "open", label: t("common.actions.open"), icon: "arrow-right", run: () => ui.nav.open(pageOf(item)) });
  } else if (isInProgress(item)) {
    out.push({ id: "resume", label: t("common.actions.resume"), icon: "play", run: () => void playItem(app, ui, item) });
    out.push({ id: "start", label: t("browse.actions.playFromStart"), icon: "rotate-ccw", run: () => void playItem(app, ui, item, { fromStart: true }) });
  } else {
    out.push({ id: "play", label: t("common.actions.play"), icon: "play", run: () => void playItem(app, ui, item) });
  }
  if (item.kind === "movie" || item.kind === "episode" || item.kind === "show" || item.kind === "season") {
    const watched = item.kind === "show" || item.kind === "season" ? (item.leaf_count ?? 0) > 0 && (item.viewed_leaf_count ?? 0) >= (item.leaf_count ?? 0) : item.user_state.watched;
    out.push({
      id: "watched",
      label: watched ? t("browse.actions.markUnwatched") : t("browse.actions.markWatched"),
      icon: watched ? "eye-off" : "circle-check",
      run: () => void setWatched(app, ui, item, !watched),
    });
  }
  if (!opts.inPanel && opts.continueWatching) {
    out.push({ id: "cw", label: t("browse.actions.removeFromContinueWatching"), icon: "x", run: () => void hideFromContinueWatching(app, ui, item) });
  }
  if (!opts.inPanel && (item.kind === "movie" || item.kind === "show")) {
    const listed = opts.detail?.in_watchlist ?? inMyList(app, item.id);
    if (listed !== null && listed !== undefined) {
      out.push({
        id: "mylist",
        label: listed ? t("browse.actions.removeFromMyList") : t("browse.actions.addToMyList"),
        icon: listed ? "bookmark-check" : "bookmark-plus",
        run: () => void setInMyList(app, ui, item, !listed),
      });
    }
  }
  if (item.kind !== "extra") {
    out.push({
      id: "playlist",
      label: t("browse.actions.addToPlaylist"),
      icon: "list-plus",
      run: () => ui.nav.open({ name: "addToPlaylist", itemIds: [item.id], title: item.kind === "episode" ? item.grandparent_title || item.title : item.title }),
    });
  }
  if (item.kind === "episode" && item.grandparent_id) out.push({ id: "show", label: t("browse.actions.goToShow"), icon: "tv", run: () => ui.nav.open({ name: "item", id: item.grandparent_id! }) });
  if (item.kind === "season" && item.parent_id) out.push({ id: "show", label: t("browse.actions.goToShow"), icon: "tv", run: () => ui.nav.open({ name: "item", id: item.parent_id! }) });
  if (item.kind === "episode" && item.parent_id) out.push({ id: "season", label: t("browse.actions.goToSeason"), icon: "layers", run: () => ui.nav.open({ name: "item", id: item.parent_id! }) });
  return out;
}

/** "S2 · E5 · Office Olympics", "2019" or "3 seasons". */
export function actionsSubtitle(item: ItemSummary): string {
  if (item.kind === "episode") return [episodeLabel(item.parent_index, item.index), item.title].filter(Boolean).join(" · ");
  return cardSubtitle(item);
}

/**
 * A tile's actions as a page of their own (TvItemActionsScreen): the
 * item's backdrop, its title, one row per action. Each row pops the page,
 * then acts.
 */
export function ItemActionsScreen(p: { id: number; continueWatching: boolean }) {
  const app = useApp();
  const ui = useUi();
  const loaded = useLoad((c) => (app.cachedItem(p.id) ? Promise.resolve(app.cachedItem(p.id)!) : ui.api.item(p.id, c)), [p.id], {
    live: false,
    initial: app.cachedItem(p.id),
  });
  const item = loaded.data;
  useEffect(() => {
    if (!item) return;
    const t = setTimeout(() => {
      const first = document.querySelector<HTMLElement>(".actions-list [data-f]");
      if (first) ui.focus.focus(first);
    }, 16);
    return () => clearTimeout(t);
  }, [!!item]);
  if (!item && loaded.error) return <Message title={failedTitle(loaded.error, t("browse.item.cantLoad"))} body={problem(loaded.error)} action={{ label: t("common.actions.back"), onSelect: () => ui.nav.pop() }} />;
  if (!item) return null;
  const actions = itemActions(app, ui, item, { continueWatching: p.continueWatching, detail: item });
  const title = item.kind === "episode" ? item.grandparent_title || item.title : item.title;
  return (
    <div class="actions-page">
      <Backdrop candidates={artCandidates(item, "backdrop")} title={item.title} scrim="item" />
      <div class="actions-column">
        <div class="t-page c-fg lines-2">{title}</div>
        <div class="t-body c-muted one-line" style="margin-top:4px">
          {actionsSubtitle(item)}
        </div>
        <G kind="col" class="actions-list">
          {actions.map((a) => (
            <PanelRow
              key={a.id}
              fid={`action/${a.id}`}
              label={a.label}
              icon={a.id === "play" || a.id === "resume" ? "play" : undefined}
              onSelect={() => {
                ui.nav.pop();
                a.run();
              }}
            />
          ))}
        </G>
      </div>
    </div>
  );
}

/** The same actions in a side panel, over the item page (a tile held, or the Play pill held). */
export function ActionsPanel(p: { item: ItemSummary; onClose: () => void }) {
  const app = useApp();
  const ui = useUi();
  const actions = itemActions(app, ui, p.item, { inPanel: true });
  return (
    <SidePanel onClose={p.onClose} label={p.item.title}>
      <PanelTitle>{p.item.title}</PanelTitle>
      {actions.map((a, i) => (
        <PanelRow
          key={a.id}
          label={a.label}
          icon={a.icon}
          initial={i === 0}
          onSelect={() => {
            p.onClose();
            a.run();
          }}
        />
      ))}
    </SidePanel>
  );
}
