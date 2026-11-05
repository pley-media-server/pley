import { useLayoutEffect, useMemo, useRef, useState } from "preact/hooks";
import type { LiveGroups } from "../../api/types";
import { currentLanguage, formatNumber, t } from "../../i18n";
import { categoryKey, groupSections, type LiveCategory } from "../../live/data";
import { useUi } from "../context";
import { TextField } from "../kit/field";
import { F, G } from "../kit/focusable";
import { Icon, type IconName } from "../kit/icons";
import { useTrap } from "../kit/panel";

/** A line's pitch in the list, and the lines drawn either side of the focused one. */
const PITCH = 44;
const DRAWN = 12;
/** Where the search sits, under the panel's title, and the list's lines under it. */
const SEARCH_TOP = 76;
const LIST_TOP = 130;

/** 57176 → "57,176" ("57 176" in French). */
export function count(n: number): string {
  return formatNumber(n);
}

type Line =
  | { kind: "category"; key: string; category: LiveCategory; label: string; icon: IconName; count: number | null }
  | { kind: "heading"; key: string; label: string; count: number }
  | { kind: "region"; key: string; region: string; groups: number; count: number; open: boolean }
  | { kind: "group"; key: string; category: LiveCategory; label: string; count: number; inRegion: boolean };

/**
 * The list's lines (the web's CategoryRail): the marked categories and
 * every channel; then the regions in alphabetical order, each opening
 * to its groups, and "More groups", the rest in the provider's order; with
 * fewer than two regions, the groups in one plain list.
 */
export function categoryLines(groups: LiveGroups | null, open: string | null): Line[] {
  const lines: Line[] = [];
  lines.push(
    { kind: "category", key: "favorites", category: { kind: "favorites" }, label: t("livetv.categories.favourites"), icon: "star", count: groups ? groups.favorites : null },
    { kind: "category", key: "recent", category: { kind: "recent" }, label: t("livetv.categories.recent"), icon: "history", count: groups ? groups.recent : null },
    { kind: "category", key: "all", category: { kind: "all" }, label: t("livetv.categories.all"), icon: "layers", count: groups ? groups.total : null },
  );
  if (!groups || groups.groups.length < 2) return lines;
  const group = (name: string, label: string, n: number, inRegion: boolean): Line => ({
    kind: "group",
    key: `group:${name}`,
    category: { kind: "group", group: name },
    label,
    count: n,
    inRegion,
  });
  const { regions, others } = groupSections(groups.groups);
  if (!regions.length) {
    lines.push({ kind: "heading", key: "heading", label: t("livetv.categories.groups"), count: others.length });
    for (const g of others) lines.push(group(g.name, g.name, g.count, false));
    return lines;
  }
  lines.push({ kind: "heading", key: "heading:regions", label: t("livetv.categories.regions"), count: regions.length });
  for (const r of regions) {
    const isOpen = open === r.region;
    lines.push({ kind: "region", key: `region:${r.region}`, region: r.region, groups: r.groups.length, count: r.count, open: isOpen });
    if (isOpen) for (const g of r.groups) lines.push(group(g.name, g.label, g.count, true));
  }
  if (others.length) {
    lines.push({ kind: "heading", key: "heading:more", label: t("livetv.categories.moreGroups"), count: others.length });
    for (const g of others) lines.push(group(g.name, g.name, g.count, false));
  }
  return lines;
}

/**
 * Live TV's categories, in a panel at the left:
 * Search at its head (the TV's keyboard; every channel, whatever category
 * was open; Left goes to it from anywhere in the list), then Favourites,
 * Recently watched and All channels, the regions and the other groups.
 * The one shown has focus as it opens (the search, when it is one); OK on
 * a region opens it (one at a time), on another line shows it; Back or
 * Right closes the panel. Only the lines near the focused one are drawn.
 */
export function CategoryPanel(p: {
  groups: LiveGroups | null;
  selected: LiveCategory;
  left: number;
  onSelect: (c: LiveCategory) => void;
  onClose: () => void;
}) {
  const ui = useUi();
  const trap = useTrap();
  const selectedKey = categoryKey(p.selected);
  const selectedGroup = p.selected.kind === "group" ? p.selected.group : null;
  const selectedRegion = useMemo(() => {
    if (!selectedGroup || !p.groups) return null;
    const r = groupSections(p.groups.groups).regions.filter((x) => x.groups.some((g) => g.name === selectedGroup))[0];
    return r ? r.region : null;
  }, [p.groups, selectedGroup]);
  const [open, setOpen] = useState<string | null>(selectedRegion);
  const lines = useMemo(() => categoryLines(p.groups, open), [p.groups, open, currentLanguage()]);
  const searching = p.selected.kind === "search";
  const startAt = Math.max(0, lines.findIndex((l) => l.key === selectedKey));
  const searchBox = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState(startAt);
  const [inside, setInside] = useState(false);
  const [words, setWords] = useState(p.selected.kind === "search" ? p.selected.q : "");
  useLayoutEffect(() => {
    const t = setTimeout(() => setInside(true), 16);
    const off = ui.input.push((key, e) => {
      if (key === "back" || key === "right") {
        if (e.type === "down" && !e.repeat) p.onClose();
        return true;
      }
      const field = searchBox.current?.querySelector<HTMLElement>("[data-f]") ?? null;
      if (key === "left" && field && ui.focus.current !== field) {
        if (e.type === "down") ui.focus.focus(field);
        return true;
      }
      return false;
    });
    return () => {
      clearTimeout(t);
      off();
    };
  }, []);

  const height = 540 - LIST_TOP;
  const shift = Math.max(0, Math.min(lines.length * PITCH - height + 12, at * PITCH - height * 0.35));
  const rows = [];
  for (let i = Math.max(0, at - DRAWN); i < Math.min(lines.length, at + DRAWN + 4); i++) {
    const l = lines[i]!;
    const top = `top:${i * PITCH}px`;
    if (l.kind === "heading") {
      rows.push(
        <div key={l.key} class="live-cat-heading t-caption" style={top}>
          <span>{l.label}</span>
          <span>{count(l.count)}</span>
        </div>,
      );
      continue;
    }
    const current = l.kind !== "region" && l.key === selectedKey;
    const holds = l.kind === "region" && !l.open && selectedRegion === l.region;
    rows.push(
      <F
        key={l.key}
        class={`live-cat${current || holds ? " current" : ""}${l.kind === "group" && l.inRegion ? " inner" : ""}`}
        fid={`cat/${l.key}`}
        initial={!searching && i === startAt}
        label={l.kind === "region" ? t("livetv.categories.region", { region: l.region, groups: t("livetv.categories.groupCount", { count: l.groups }) }) : l.label}
        style={top}
        onFocus={() => setAt(i)}
        onSelect={() => {
          if (l.kind === "region") setOpen((o) => (o === l.region ? null : l.region));
          else p.onSelect(l.category);
        }}
      >
        {l.kind === "category" && <Icon name={l.icon} size={18} class="live-cat-icon" />}
        {l.kind === "region" && <Icon name={l.open ? "chevron-down" : "chevron-right"} size={16} class="live-cat-icon" />}
        {l.kind === "region" ? (
          <>
            <span class="live-cat-region t-badge">{l.region}</span>
            <span class="t-caption live-cat-sub one-line">{t("livetv.categories.groupCount", { count: l.groups })}</span>
          </>
        ) : (
          <span class="t-label one-line live-cat-label">{l.label}</span>
        )}
        {l.count !== null && <span class="t-caption live-cat-count">{count(l.count)}</span>}
      </F>,
    );
  }
  return (
    <div ref={trap} class="panel-root" style="position:absolute;left:0;top:0;right:0;bottom:0;z-index:30" aria-label={t("livetv.categories.label")}>
      <div class={`scrim${inside ? " in" : ""}`} onClick={p.onClose} />
      <div class={`live-cats${inside ? " in" : ""}`} style={`left:${p.left}px`}>
        <div class="t-title c-fg live-cats-title">{t("livetv.categories.title")}</div>
        <div ref={searchBox} class="live-cats-search" style={`top:${SEARCH_TOP}px`}>
          <TextField
            value={words}
            placeholder={t("livetv.categories.search")}
            icon="search"
            type="search"
            fid="cat/search"
            initial={searching}
            onInput={setWords}
            onSubmit={(q) => {
              const w = q.trim();
              if (w) p.onSelect({ kind: "search", q: w });
            }}
          />
        </div>
        <div class="live-cats-box" style={`top:${LIST_TOP}px`}>
          <G kind="col" class="live-cats-rows" style={`height:${lines.length * PITCH}px;transform:translateY(${-Math.max(0, shift)}px)`}>
            {rows}
          </G>
        </div>
      </div>
    </div>
  );
}
