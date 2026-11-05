import { useMemo, useState } from "react";
import { ChevronRight, History, LayoutGrid, Star } from "lucide-react";
import { useTranslation } from "react-i18next";
import { categoryKey, type LiveCategory, type LiveGroup, type LiveGroups } from "@/api/livetv";
import { Skeleton } from "@/components/ui";
import { currentLocale, t } from "@/i18n";
import { cn } from "@/lib/cn";

export interface Region {
  region: string;
  groups: LiveGroup[];
  count: number;
}

/**
 * The groups as the side shows them. A provider naming hundreds of groups
 * by region ("UK| Sport", "UK| News"…) has them gathered under each region,
 * the regions in alphabetical order, and the rest after, in its own order;
 * a region with one group is one of the rest. Without regions, the groups
 * as the provider lists them.
 */
export function groupSections(groups: LiveGroup[]): { regions: Region[]; others: LiveGroup[] } {
  const byRegion = new Map<string, LiveGroup[]>();
  for (const g of groups) {
    if (g.region) byRegion.set(g.region, [...(byRegion.get(g.region) ?? []), g]);
  }
  const regions = [...byRegion]
    .filter(([, gs]) => gs.length > 1)
    .map(([region, gs]) => ({ region, groups: gs, count: gs.reduce((n, g) => n + g.count, 0) }))
    .sort((a, b) => a.region.localeCompare(b.region));
  if (regions.length < 2) return { regions: [], others: groups };
  const gathered = new Set(regions.map((r) => r.region));
  return { regions, others: groups.filter((g) => !g.region || !gathered.has(g.region)) };
}

/** What a category is called: "Favourites", "UK · Sport". */
export function categoryName(c: LiveCategory, groups: LiveGroup[] | undefined): string {
  switch (c.kind) {
    case "all":
      return t("categories.all", { ns: "livetv" });
    case "favorites":
      return t("categories.favourites", { ns: "livetv" });
    case "recent":
      return t("categories.recent", { ns: "livetv" });
    case "group": {
      const g = groups?.find((x) => x.name === c.group);
      return g?.region ? `${g.region} · ${g.label}` : (g?.label ?? c.group);
    }
  }
}

const count = (n: number) => n.toLocaleString(currentLocale());

/** The categories down the side of Live TV: the marked ones, all, then the regions and groups. */
export function CategoryRail({
  groups,
  selected,
  onSelect,
  className,
}: {
  groups: LiveGroups | undefined;
  selected: LiveCategory;
  onSelect: (c: LiveCategory) => void;
  className?: string;
}) {
  const { t } = useTranslation("livetv");
  const { regions, others } = useMemo(() => groupSections(groups?.groups ?? []), [groups]);
  const selectedRegion =
    selected.kind === "group" ? (regions.find((r) => r.groups.some((g) => g.name === selected.group))?.region ?? null) : null;
  const [open, setOpen] = useState<string | null>(selectedRegion);
  const key = categoryKey(selected);

  return (
    <nav aria-label={t("categories.label")} className={cn("flex min-h-0 flex-col", className)}>
      <div className="space-y-0.5 border-b border-border p-2">
        <Item active={key === "favorites"} onClick={() => onSelect({ kind: "favorites" })} icon={<Star className="size-4" />} count={groups?.favorites}>
          {t("categories.favourites")}
        </Item>
        <Item active={key === "recent"} onClick={() => onSelect({ kind: "recent" })} icon={<History className="size-4" />} count={groups?.recent}>
          {t("categories.recent")}
        </Item>
        <Item active={key === "all"} onClick={() => onSelect({ kind: "all" })} icon={<LayoutGrid className="size-4" />} count={groups?.total}>
          {t("categories.all")}
        </Item>
      </div>
      {!groups ? (
        <div className="space-y-2 p-3">
          {Array.from({ length: 8 }, (_, i) => (
            <Skeleton key={i} className="h-7 w-full rounded-md" />
          ))}
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-2 pb-3 [scrollbar-width:thin]">
          {regions.length ? (
            <>
              <Heading>{t("categories.regions")}</Heading>
              <div className="space-y-0.5">
                {regions.map((r) => (
                  <RegionItem
                    key={r.region}
                    region={r}
                    open={open === r.region}
                    onToggle={() => setOpen((o) => (o === r.region ? null : r.region))}
                    selectedKey={key}
                    onSelect={onSelect}
                  />
                ))}
              </div>
            </>
          ) : null}
          {others.length > 1 || (others.length === 1 && regions.length) ? (
            <>
              <Heading>{regions.length ? t("categories.moreGroups") : t("categories.groups")}</Heading>
              <div className="space-y-0.5">
                {others.map((g) => (
                  <Item key={g.name} active={key === `group:${g.name}`} onClick={() => onSelect({ kind: "group", group: g.name })} count={g.count}>
                    {g.name || t("categories.noGroup")}
                  </Item>
                ))}
              </div>
            </>
          ) : null}
        </div>
      )}
    </nav>
  );
}

function Heading({ children }: { children: React.ReactNode }) {
  return <div className="px-2.5 pb-1 pt-4 text-[11px] font-semibold uppercase tracking-wider text-subtle">{children}</div>;
}

function RegionItem({
  region: r,
  open,
  onToggle,
  selectedKey,
  onSelect,
}: {
  region: Region;
  open: boolean;
  onToggle: () => void;
  selectedKey: string;
  onSelect: (c: LiveCategory) => void;
}) {
  const holdsSelected = r.groups.some((g) => selectedKey === `group:${g.name}`);
  return (
    <div className={cn("rounded-md", open && "bg-surface-2/60")}>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        className={cn(
          "flex min-h-8 w-full items-center gap-2.5 rounded-md px-2.5 py-1 text-left text-sm transition-colors hover:bg-surface-2",
          holdsSelected ? "text-accent" : "text-muted hover:text-fg",
        )}
      >
        <span className={cn("min-w-0 flex-1 truncate font-medium", holdsSelected ? "text-accent" : "text-fg")}>{r.region}</span>
        <span className="shrink-0 text-xs tabular-nums text-subtle">{count(r.count)}</span>
        <ChevronRight className={cn("size-3.5 shrink-0 text-subtle transition-transform duration-200", open && "rotate-90")} />
      </button>
      {open ? (
        <div className="space-y-0.5 pb-1.5 pl-3">
          {r.groups.map((g) => (
            <Item key={g.name} active={selectedKey === `group:${g.name}`} onClick={() => onSelect({ kind: "group", group: g.name })} count={g.count}>
              {g.label}
            </Item>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function Item({
  active,
  onClick,
  icon,
  count: n,
  children,
}: {
  active: boolean;
  onClick: () => void;
  icon?: React.ReactNode;
  count?: number;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "true" : undefined}
      className={cn(
        "flex min-h-8 w-full items-center gap-2.5 rounded-md px-2.5 py-1 text-left text-sm transition-colors",
        active ? "bg-accent/15 font-medium text-accent" : "text-muted hover:bg-surface-2 hover:text-fg",
      )}
    >
      {icon ? <span className="shrink-0">{icon}</span> : null}
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {n !== undefined ? <span className={cn("shrink-0 text-xs tabular-nums", active ? "text-accent/80" : "text-subtle")}>{count(n)}</span> : null}
    </button>
  );
}
