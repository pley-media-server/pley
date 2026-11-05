import { useMemo, useState } from "react";
import { ChevronRight, Eye, EyeOff, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useLineupGroups, usePatchLineup, type LineupGroup } from "@/api/livetv";
import { useToast } from "@/components/Toast";
import { Button, EmptyState, PageSpinner, Select, TextInput, Toggle } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";

type Shown = "all" | "shown" | "hidden";

type Section = { region: string | null; groups: LineupGroup[] };

/** Groups by region in the lineup's order; groups of no region (or a region of one) on their own. */
function sections(groups: LineupGroup[]): Section[] {
  const byRegion = new Map<string, LineupGroup[]>();
  for (const g of groups) {
    if (g.region) byRegion.set(g.region, [...(byRegion.get(g.region) ?? []), g]);
  }
  const out: Section[] = [];
  const placed = new Set<string>();
  for (const g of groups) {
    const region = g.region && (byRegion.get(g.region)?.length ?? 0) > 1 ? g.region : null;
    if (!region) out.push({ region: null, groups: [g] });
    else if (!placed.has(region)) {
      placed.add(region);
      out.push({ region, groups: byRegion.get(region)! });
    }
  }
  return out;
}

const hiddenAll = (gs: LineupGroup[]) => gs.every((g) => g.hidden === g.count);
const shownAll = (gs: LineupGroup[]) => gs.every((g) => g.hidden === 0);

/**
 * Settings → Live TV → Groups: whole groups, and whole regions of them,
 * shown or hidden at once. A provider's hundreds of groups come down to
 * the ones the household watches.
 */
export function Groups() {
  const { t } = useTranslation(["livetv", "common"]);
  const groups = useLineupGroups();
  const patch = usePatchLineup();
  const toast = useToast();
  const [search, setSearch] = useState("");
  const [shown, setShown] = useState<Shown>("all");
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const all = groups.data;
  const q = search.trim().toLowerCase();
  const listed = useMemo(
    () =>
      sections(
        (all ?? []).filter(
          (g) =>
            (!q || g.name.toLowerCase().includes(q)) &&
            (shown === "all" || (shown === "hidden" ? g.hidden === g.count : g.hidden < g.count)),
        ),
      ),
    [all, q, shown],
  );

  if (groups.isPending) return <PageSpinner />;
  if (groups.isError) {
    return (
      <EmptyState
        title={t("settings.groups.couldntLoad")}
        description={errorMessage(groups.error)}
        action={<Button onClick={() => void groups.refetch()}>{t("common:actions.retry")}</Button>}
      />
    );
  }
  if (!all?.length) return <EmptyState title={t("settings.groups.noneTitle")} description={t("settings.groups.noneHint")} />;

  // What the toast says was hidden or shown: a region's groups, or one group.
  const setHidden = (names: string[], hidden: boolean, what: { region: string } | { name: string }) =>
    patch.mutate(
      { ids: [], groups: names, hidden },
      {
        onSuccess: () =>
          toast.success(
            "region" in what
              ? t(hidden ? "settings.groups.regionHidden" : "settings.groups.regionShown", { region: what.region })
              : what.name
                ? t(hidden ? "settings.groups.groupHidden" : "settings.groups.groupShown", { name: what.name })
                : t(hidden ? "settings.groups.unnamedHidden" : "settings.groups.unnamedShown"),
          ),
        onError: (e) => toast.error(errorMessage(e)),
      },
    );
  const channels = all.reduce((n, g) => n + g.count, 0);
  const hiddenChannels = all.reduce((n, g) => n + g.hidden, 0);
  const hiddenGroups = all.filter((g) => g.hidden === g.count).length;
  const toggle = (region: string) =>
    setOpen((o) => {
      const next = new Set(o);
      if (next.has(region)) next.delete(region);
      else next.add(region);
      return next;
    });

  return (
    <div className="space-y-4">
      <div>
        <h3 className="text-base font-semibold">{t("settings.groups.title")}</h3>
        <p className="text-sm text-muted">
          {t("settings.groups.shown", { count: all.length - hiddenGroups })}
          {hiddenGroups ? t("settings.groups.alsoHidden", { count: hiddenGroups }) : ""} ·{" "}
          {t("settings.groups.toWatch", { count: channels - hiddenChannels })} {t("settings.groups.hint")}
        </p>
      </div>
      <div className="flex flex-wrap gap-2">
        <div className="relative min-w-48 flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-subtle" />
          <TextInput
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("settings.groups.find")}
            aria-label={t("settings.groups.find")}
            className="h-9 pl-9"
          />
        </div>
        <Select value={shown} onChange={(e) => setShown(e.target.value as Shown)} aria-label={t("settings.groups.which")}>
          <option value="all">{t("settings.groups.shownAndHidden")}</option>
          <option value="shown">{t("settings.groups.shownOnly")}</option>
          <option value="hidden">{t("settings.groups.hiddenOnly")}</option>
        </Select>
      </div>

      <div className="overflow-hidden rounded-lg border border-border bg-surface">
        {listed.length === 0 ? (
          <p className="p-6 text-center text-sm text-muted">{t("settings.groups.noMatch")}</p>
        ) : (
          <ul className="max-h-[min(40rem,68dvh)] divide-y divide-border/60 overflow-y-auto [scrollbar-width:thin]">
            {listed.map((s) =>
              s.region ? (
                <li key={`r:${s.region}`}>
                  <div className="flex h-12 items-center gap-2 px-2 sm:px-3">
                    <button
                      type="button"
                      onClick={() => toggle(s.region!)}
                      aria-expanded={open.has(s.region) || !!q}
                      className="flex min-w-0 flex-1 items-center gap-2.5 rounded-md py-1.5 text-left"
                    >
                      <ChevronRight className={cn("size-4 shrink-0 text-subtle transition-transform", (open.has(s.region) || q) && "rotate-90")} />
                      <span className="min-w-9 rounded bg-surface-3 px-1.5 py-0.5 text-center text-xs font-bold tracking-wide">{s.region}</span>
                      <span className={cn("truncate text-sm", hiddenAll(s.groups) && "text-subtle")}>
                        {t("settings.groups.regionGroups", { count: s.groups.length })} ·{" "}
                        {t("settings.groups.regionChannels", { count: s.groups.reduce((n, g) => n + g.count, 0) })}
                      </span>
                      {!hiddenAll(s.groups) && !shownAll(s.groups) ? (
                        <span className="hidden shrink-0 text-xs text-subtle sm:inline">{t("settings.groups.someHidden")}</span>
                      ) : null}
                    </button>
                    <Button
                      size="sm"
                      variant="ghost"
                      icon={hiddenAll(s.groups) ? <Eye className="size-4" /> : <EyeOff className="size-4" />}
                      onClick={() => setHidden(s.groups.map((g) => g.name), !hiddenAll(s.groups), { region: s.region! })}
                      disabled={patch.isPending}
                    >
                      {hiddenAll(s.groups) ? t("settings.groups.showAll") : t("settings.groups.hideAll")}
                    </Button>
                  </div>
                  {open.has(s.region) || q ? (
                    <ul className="mb-2 ml-6 border-l border-border pl-2 sm:ml-8">
                      {s.groups.map((g) => (
                        <GroupRow key={g.name} group={g} label={g.label} busy={patch.isPending} onHidden={(h) => setHidden([g.name], h, { name: g.name })} />
                      ))}
                    </ul>
                  ) : null}
                </li>
              ) : (
                <GroupRow key={s.groups[0]!.name} group={s.groups[0]!} label={s.groups[0]!.name} busy={patch.isPending} onHidden={(h) => setHidden([s.groups[0]!.name], h, { name: s.groups[0]!.name })} padded />
              ),
            )}
          </ul>
        )}
      </div>
    </div>
  );
}

function GroupRow({
  group: g,
  label,
  busy,
  onHidden,
  padded,
}: {
  group: LineupGroup;
  label: string;
  busy: boolean;
  onHidden: (hidden: boolean) => void;
  padded?: boolean;
}) {
  const { t } = useTranslation("livetv");
  const hidden = g.hidden === g.count;
  const name = label || t("categories.noGroup");
  return (
    <li className={cn("flex h-11 items-center gap-3 pr-2 sm:pr-3", padded ? "pl-4 sm:pl-5" : "pl-2")}>
      <span className={cn("min-w-0 flex-1 truncate text-sm", hidden && "text-subtle line-through decoration-subtle/40")}>{name}</span>
      <span className="shrink-0 text-xs tabular-nums text-subtle">
        {[
          g.hidden > 0 && !hidden ? t("settings.groups.hiddenCount", { count: g.hidden }) : null,
          g.offline > 0 ? t("settings.groups.offlineCount", { count: g.offline }) : null,
          g.count.toLocaleString(currentLocale()),
        ]
          .filter(Boolean)
          .join(" · ")}
      </span>
      <Toggle checked={!hidden} onChange={(show) => onHidden(!show)} label={t("settings.showName", { name })} disabled={busy} />
    </li>
  );
}
