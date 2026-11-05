import { useState } from "preact/hooks";
import { errorMessage } from "../../api/client";
import type { Collection, ItemSummary, Library } from "../../api/types";
import { pluralize } from "../../format/format";
import { t } from "../../i18n";
import { useUi } from "../context";
import { Backdrop } from "../kit/backdrop";
import { Pill, Round } from "../kit/buttons";
import { G } from "../kit/focusable";
import { PosterGrid } from "../kit/grid";
import { Icon } from "../kit/icons";
import { Row, rowHeight, SectionPage, type Section } from "../kit/row";
import { ArtTile, Tile } from "../kit/tile";
import type { Entry } from "../shell/navigator";
import { ColumnMessage, failedTitle, PageBadge, problem, sentence, useApp, useLoad } from "./common";
import { openPlayer, pageOf } from "./item-actions";
import { collectionCount, LibrariesFailed } from "./library";

const ROWS_TOP = 80;

/**
 * Collections (the rail's TvCollectionsScreen): a row per movie or show
 * library that has some.
 */
export function CollectionsScreen(p: { entry: Entry; libraryId?: number }) {
  const app = useApp();
  const ui = useUi();
  const libraries = (app.libraries.value ?? []).filter((l) => l.kind !== "music");
  const loaded = useLoad(
    (c) => Promise.all(libraries.map((l) => ui.api.collections(l.id, c).then((r) => ({ library: l, collections: r.collections })))),
    [libraries.map((l) => l.id).join(",")],
  );
  void p.libraryId;
  if (!app.libraries.value) return <LibrariesFailed top={ROWS_TOP} />;
  const groups = (loaded.data ?? []).filter((g) => g.collections.length > 0);
  if (!loaded.data && loaded.error) {
    return <ColumnMessage top={ROWS_TOP} title={failedTitle(loaded.error, t("browse.collections.cantLoad"))} body={problem(loaded.error)} action={{ label: t("common.actions.retry"), icon: "refresh-cw", onSelect: loaded.reload }} />;
  }
  if (loaded.data && !groups.length) {
    return <ColumnMessage top={ROWS_TOP} title={t("browse.collections.none")} body={t("browse.collections.noneHint")} />;
  }
  // Each row's tile focused last, so Back from a collection comes to it, the rows drawn again.
  const rowIndex = (p.entry.state.rowIndex as Record<string, number> | undefined) ?? {};
  p.entry.state.rowIndex = rowIndex;
  const sections: Section[] = groups.map((g: { library: Library; collections: Collection[] }, gi) => ({
    key: `lib-${g.library.id}`,
    height: rowHeight("poster"),
    anchor: ROWS_TOP,
    render: (onFocus) => (
      <Row
        title={g.library.name}
        kind="poster"
        items={g.collections}
        keyOf={(c) => c.id}
        enterAt={rowIndex[`lib-${g.library.id}`]}
        onLeftEdge={ui.toRail}
        onFocusIndex={(i) => {
          rowIndex[`lib-${g.library.id}`] = i;
          onFocus();
        }}
        tile={(c, i, place) => (
          <ArtTile
            kind="poster"
            title={c.title}
            subtitle={collectionCount(c, g.library.kind)}
            candidates={[c.poster_url]}
            fid={`collections/${c.id}`}
            initial={gi === 0 && i === 0}
            priority={place.priority}
            onFocus={place.onFocus}
            onSelect={() => ui.nav.open({ name: "collection", id: c.id })}
          />
        )}
      />
    ),
  }));
  return (
    <div class="collections-page">
      <SectionPage
        sections={sections}
        top={ROWS_TOP}
        clipAbove={ROWS_TOP - 12}
        focusedKey={p.entry.state.section as string | undefined}
        onFocusSection={(k) => (p.entry.state.section = k)}
      />
      <div class="t-page c-fg" style="position:absolute;left:96px;top:27px">
        {t("browse.collections.title")}
      </div>
      <div class="rail-fade" />
    </div>
  );
}

type CollectionDetail = Collection & { items: ItemSummary[]; automatic?: boolean; smart_filter?: string | null; sort?: string | null };

const HERO_H = 436;

/**
 * A collection (TvCollectionScreen): its backdrop and hero, Play and
 * Shuffle, then its titles as a grid.
 */
export function CollectionScreen(p: { entry: Entry; id: number }) {
  const app = useApp();
  const ui = useUi();
  const loaded = useLoad((c) => ui.api.collection(p.id, c) as Promise<CollectionDetail>, [p.id]);
  const [problemText, setProblemText] = useState<string | null>(null);
  const col = loaded.data;
  if (!col) {
    if (loaded.error) return <ColumnMessage top={58} title={failedTitle(loaded.error, t("browse.collections.cantLoadOne"))} body={problem(loaded.error)} action={{ label: t("common.actions.retry"), icon: "refresh-cw", onSelect: loaded.reload }} />;
    return (
      <div class="collection-page" role="status" aria-label={t("common.states.loading")}>
        {[
          [58, 120, 18],
          [90, 360, 44],
          [148, 240, 18],
          [180, 464, 70],
        ].map(([y, w, h]) => (
          <div key={y} class="skeleton" style={`position:absolute;left:96px;top:${y}px;width:${w}px;height:${h}px`} />
        ))}
        <div class="skeleton" style="position:absolute;left:96px;top:270px;width:128px;height:40px;border-radius:6px" />
      </div>
    );
  }
  const library = (app.libraries.value ?? []).filter((l) => l.id === col.library_id)[0] ?? null;
  const items = col.items;
  const shows = items.length > 0 && items[0]!.kind === "show";
  const say = (text: string) => {
    setProblemText(text);
    setTimeout(() => setProblemText(null), 4000);
  };
  const play = async (shuffle: boolean) => {
    if (!items.length) return;
    const pick = shuffle ? items[Math.floor(Math.random() * items.length)]! : items[0]!;
    if (!shows) {
      openPlayer(ui, { itemId: pick.id, queue: { kind: "collection", id: col.id } });
      return;
    }
    try {
      const { item } = await ui.api.onDeck(pick.id);
      if (!item) say(t("browse.collections.noEpisodes"));
      else openPlayer(ui, { itemId: item.id });
    } catch (e) {
      say(sentence(errorMessage(e)));
    }
  };
  const notes = col.smart_filter
    ? describeFilter(col.smart_filter)
    : col.sort === "release"
      ? t("browse.collections.byRelease")
      : col.sort === "title"
        ? t("browse.collections.byTitle")
        : col.sort === "custom"
          ? t("browse.collections.customOrder")
          : "";
  const facts = [library ? collectionCount(col, library.kind) : pluralize(col.item_count, "title"), library?.name ?? "", notes].filter(Boolean).join("  ·  ");
  const hero = (
    <div class="collection-hero">
      <div class="collection-eyebrow">
        <Icon name="layers" size={18} color="var(--muted)" />
        <span class="t-label c-muted">{t("browse.collections.label")}</span>
        {col.automatic && <PageBadge text="TMDB" />}
        {col.smart_filter && <PageBadge text={t("browse.smart")} />}
      </div>
      <div class="t-hero c-fg lines-2" style="margin-top:10px">
        {col.title}
      </div>
      <div class="t-label c-muted one-line" style="margin-top:12px">
        {facts}
      </div>
      {col.summary && <div class="t-body-large lines-3 hero-summary" style="margin-top:12px">{col.summary}</div>}
      <div style="flex:1" />
      <G kind="row" class="buttons">
        <Pill label={t("common.actions.play")} icon="play" fid="play" initial={items.length > 0} disabled={!items.length} onSelect={() => void play(false)} />
        <Round label={t("browse.actions.shuffle")} icon="shuffle" fid="shuffle" initial={!items.length} disabled={items.length < 2} onSelect={() => void play(true)} />
        {problemText && <div class="t-label c-danger" style="margin:14px 0 0 12px">{problemText}</div>}
      </G>
      <div style="height:36px" />
      {!items.length && <div class="t-body c-muted">{t("browse.collections.empty")}</div>}
    </div>
  );
  return (
    <div class="collection-page">
      <Backdrop candidates={[col.backdrop_url || col.poster_url].filter(Boolean)} title={col.title} scrim="item" />
      <PosterGrid
        kind="grid"
        count={items.length}
        top={58}
        clipTop={0}
        hero={hero}
        heroHeight={HERO_H}
        cell={(i, place) => {
          const it = items[i]!;
          return (
            <Tile
              item={it}
              kind="grid"
              fid={`grid/${it.id}`}
              priority={place.priority}
              onFocus={place.onFocus}
              onSelect={() => ui.nav.open(pageOf(it))}
              onActions={() => ui.nav.open({ name: "actions", id: it.id })}
            />
          );
        }}
      />
      <div class="rail-fade" />
    </div>
  );
}

/** A smart filter in words: "Genre: Comedy", "Unwatched", "From 2010", "Newest first". */
export function describeFilter(filter: string): string {
  const out: string[] = [];
  for (const part of filter.split("&")) {
    const [k, v = ""] = part.split("=").map((x) => decodeURIComponent(x.replace(/\+/g, " ")));
    if (k === "genre" && v) out.push(t("browse.smartFilter.genre", { value: v }));
    else if (k === "unwatched") out.push(v === "1" ? t("browse.smartFilter.unwatched") : t("browse.smartFilter.watched"));
    else if (k === "year" && v) out.push(t("browse.smartFilter.year", { value: v }));
    else if (k === "sort" && /desc$/.test(v)) out.push(t("browse.smartFilter.sort.year.desc"));
  }
  return out.join("  ·  ");
}
