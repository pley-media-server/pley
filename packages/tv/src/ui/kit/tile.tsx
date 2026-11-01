import type { ItemSummary } from "../../api/types";
import { cardSubtitle, episodeLabel, formatRuntime, formatTimeLeft, isInProgress, progressFraction, summaryBadges, unwatchedCount } from "../../format/format";
import { Art, artCandidates, type ArtKind } from "./art";
import { F } from "./focusable";
import { Icon, type IconName } from "./icons";
import { memo } from "./memo";

/** The Android app's TV tile kinds: well size, gap in a row, shape, picture. */
export type TileKind = "poster" | "landscape" | "episode" | "grid" | "square" | "person" | "avatar";

export const TILES: Record<TileKind, { w: number; h: number; gap: number; round: boolean; art: ArtKind }> = {
  poster: { w: 108, h: 162, gap: 14, round: false, art: "poster" },
  landscape: { w: 204, h: 115, gap: 18, round: false, art: "thumb" },
  episode: { w: 184, h: 104, gap: 18, round: false, art: "thumb" },
  grid: { w: 96, h: 144, gap: 14, round: false, art: "poster" },
  square: { w: 132, h: 132, gap: 14, round: false, art: "poster" },
  person: { w: 96, h: 96, gap: 14, round: true, art: "poster" },
  avatar: { w: 100, h: 100, gap: 14, round: true, art: "poster" },
};

/** What a tile shows of an item (the Android app's ArtworkTileModel). */
export interface TileModel {
  title: string;
  subtitle: string;
  progress: number;
  unwatched: number;
  watched: boolean;
  badges: string[];
}

export function tileModel(item: ItemSummary, kind: TileKind, opts: { badges?: boolean; subtitle?: string } = {}): TileModel {
  const inProgress = isInProgress(item);
  const left = inProgress ? formatTimeLeft(item.user_state.position_ms, item.duration_ms) : "";
  let title = item.title;
  let subtitle = "";
  if (kind === "episode") {
    title = item.index != null ? `${item.index}. ${item.title}` : item.title;
    subtitle = left || formatRuntime(item.duration_ms);
  } else if (kind === "landscape" && item.kind === "episode") {
    title = item.grandparent_title || item.title;
    const episode = episodeLabel(item.parent_index, item.index);
    const rest = left || (item.title !== title ? item.title : "");
    subtitle = [episode, rest].filter(Boolean).join(" · ");
  } else {
    subtitle = left || cardSubtitle(item);
  }
  const unwatched = item.kind === "show" || item.kind === "season" ? unwatchedCount(item) : 0;
  return {
    title,
    subtitle: opts.subtitle ?? subtitle,
    progress: inProgress ? progressFraction(item.user_state.position_ms, item.duration_ms) : 0,
    unwatched,
    watched: item.user_state.watched && unwatched === 0,
    badges: opts.badges ? summaryBadges(item.media) : [],
  };
}

export interface TileProps {
  item: ItemSummary;
  kind: TileKind;
  fid?: string;
  initial?: boolean;
  badges?: boolean;
  /** Replaces the tile's own subtitle. */
  subtitle?: string;
  /** Pictures for places that aren't items (a collection, a playlist). */
  candidates?: string[];
  onSelect?: () => void;
  onActions?: () => void;
  onFocus?: (el: HTMLElement) => void;
  style?: string;
  priority?: number;
}

/**
 * A tile: the well (picture, overlays, focus ring) and its two-line
 * caption. Drawn again only when what it shows changes; its `priority`
 * counts once, when its picture is first asked for.
 */
export const Tile = memo(TileView, ["priority"]);

function TileView(p: TileProps) {
  const spec = TILES[p.kind];
  const m = tileModel(p.item, p.kind, { badges: p.badges, subtitle: p.subtitle });
  return (
    <F
      class="tile"
      style={`width:${spec.w}px;${p.style ?? ""}`}
      fid={p.fid}
      initial={p.initial}
      label={[m.title, m.subtitle].filter(Boolean).join(", ")}
      onSelect={p.onSelect}
      onLongSelect={p.onActions}
      onMenu={p.onActions}
      onFocus={p.onFocus}
    >
      <div class={`tile-well${spec.round ? " round" : ""}`} style={`width:${spec.w}px;height:${spec.h}px`}>
        <div class="tile-clip">
          <Art
            candidates={p.candidates ?? artCandidates(p.item, spec.art)}
            width={spec.w}
            title={p.item.title}
            showTitle={!spec.round}
            class="tile-clip"
            priority={p.priority}
          />
          {!spec.round && <div class="tile-hairline" />}
          {m.progress > 0 && (
            <div class="tile-progress">
              <div style={`width:${(m.progress * 100).toFixed(2)}%`} />
            </div>
          )}
          {m.badges.length > 0 && (
            <div class={`tile-badges${m.progress > 0 ? " above-progress" : ""}`}>
              {m.badges.map((b) => (
                <div key={b} class="tile-badge t-badge">
                  {b}
                </div>
              ))}
            </div>
          )}
          {m.unwatched > 0 ? (
            <div class="tile-count t-badge">{m.unwatched}</div>
          ) : m.watched ? (
            <div class="tile-tick">
              <Icon name="check" size={12} />
            </div>
          ) : null}
        </div>
        <div class="tile-ring" />
      </div>
      <TileCaption title={m.title} subtitle={m.subtitle} />
    </F>
  );
}

export function TileCaption(p: { title: string; subtitle: string }) {
  return (
    <div class="tile-caption" aria-hidden="true">
      <div class="title t-caption one-line">{p.title}</div>
      {p.subtitle && <div class="sub t-caption one-line">{p.subtitle}</div>}
    </div>
  );
}

/**
 * The last tile of a row that lists more: an arrow and "See all" (or where
 * it goes: "See all on Discover"), or another icon and words (Show more).
 */
export const SeeAllTile = memo(SeeAllView);

function SeeAllView(p: {
  kind: TileKind;
  fid?: string;
  label?: string;
  icon?: IconName;
  initial?: boolean;
  onSelect: () => void;
  onFocus?: (el: HTMLElement) => void;
  style?: string;
}) {
  const spec = TILES[p.kind];
  const label = p.label ?? "See all";
  return (
    <F class="tile" style={`width:${spec.w}px;${p.style ?? ""}`} fid={p.fid} initial={p.initial} label={label} onSelect={p.onSelect} onFocus={p.onFocus}>
      <div class={`tile-well see-all${spec.round ? " round" : ""}`} style={`width:${spec.w}px;height:${spec.h}px`}>
        <Icon name={p.icon ?? "arrow-right"} size={32} />
        <div class="t-label">{label}</div>
        <div class="tile-ring" />
      </div>
      <div class="tile-caption" />
    </F>
  );
}

/** A tile's place while its item loads: the well and two caption bars. */
export const SkeletonTile = memo(SkeletonView);

function SkeletonView(p: { kind: TileKind; fid?: string; style?: string; focusable?: boolean; onFocus?: (el: HTMLElement) => void }) {
  const spec = TILES[p.kind];
  const body = (
    <>
      <div class={`tile-well${spec.round ? " round" : ""}`} style={`width:${spec.w}px;height:${spec.h}px`}>
        <div class="tile-ring" />
      </div>
      <div style="height:34px;margin-top:14px">
        <div class="skeleton" style={`width:${Math.round(spec.w * 0.6)}px;height:12px;margin-top:3px`} />
        <div class="skeleton" style={`width:${Math.round(spec.w * 0.35)}px;height:12px;margin-top:8px`} />
      </div>
    </>
  );
  return p.focusable ? (
    <F class="tile" style={`width:${spec.w}px;${p.style ?? ""}`} fid={p.fid} onFocus={p.onFocus}>
      {body}
    </F>
  ) : (
    <div class="tile" style={`width:${spec.w}px;${p.style ?? ""}`}>
      {body}
    </div>
  );
}

/**
 * A tile for what isn't an item (a collection, a playlist, a person): its
 * pictures, its two caption lines, and a count badge at the top right.
 */
export const ArtTile = memo(ArtTileView, ["priority"]);

function ArtTileView(p: {
  kind: TileKind;
  title: string;
  subtitle?: string;
  candidates: string[];
  /** A count on the art's corner ("12"). */
  badge?: string | null;
  fid?: string;
  initial?: boolean;
  onSelect?: () => void;
  onActions?: () => void;
  onFocus?: (el: HTMLElement) => void;
  priority?: number;
  style?: string;
}) {
  const spec = TILES[p.kind];
  return (
    <F
      class="tile"
      style={`width:${spec.w}px;${p.style ?? ""}`}
      fid={p.fid}
      initial={p.initial}
      label={[p.title, p.subtitle].filter(Boolean).join(", ")}
      onSelect={p.onSelect}
      onLongSelect={p.onActions}
      onMenu={p.onActions}
      onFocus={p.onFocus}
    >
      <div class={`tile-well${spec.round ? " round" : ""}`} style={`width:${spec.w}px;height:${spec.h}px`}>
        <div class="tile-clip">
          <Art candidates={p.candidates} width={spec.w} title={p.title} showTitle={!spec.round} class="tile-clip" priority={p.priority} />
          {!spec.round && <div class="tile-hairline" />}
          {p.badge && <div class="tile-corner-badge t-badge">{p.badge}</div>}
        </div>
        <div class="tile-ring" />
      </div>
      <TileCaption title={p.title} subtitle={p.subtitle ?? ""} />
    </F>
  );
}
