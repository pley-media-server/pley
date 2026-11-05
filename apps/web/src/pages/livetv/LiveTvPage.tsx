import { useState } from "react";
import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { CalendarDays, ChevronDown, LayoutGrid, RefreshCw, Search, Settings2, Tv2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import {
  categoryKey,
  liveQueries,
  parseCategory,
  useLiveGroups,
  useLiveSummary,
  type Channel,
  type LiveCategory,
  type LiveGroups,
  type Program,
} from "@/api/livetv";
import { useAuth } from "@/auth";
import { Modal } from "@/components/Modal";
import { Button, buttonClass, EmptyState, Skeleton, TextInput } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { useDocumentTitle, useLocalStorage } from "@/lib/hooks";
import { CategoryRail, categoryName } from "./CategoryRail";
import { ChannelGrid } from "./ChannelGrid";
import { useNow } from "./common";
import { GuideGrid } from "./GuideGrid";
import { ProgramModal } from "./ProgramModal";

type View = "guide" | "channels";

/** Whether `category` still has channels to show. */
function stillThere(c: LiveCategory, groups: LiveGroups): boolean {
  switch (c.kind) {
    case "all":
      return true;
    case "favorites":
      return groups.favorites > 0;
    case "recent":
      return groups.recent > 0;
    case "group":
      return groups.groups.some((g) => g.name === c.group);
  }
}

export function LiveTvPage() {
  const { t } = useTranslation(["livetv", "common"]);
  useDocumentTitle(t("liveTv"));
  const user = useAuth();
  const summary = useLiveSummary();
  const groups = useLiveGroups(summary.data?.available ?? false);
  const now = useNow();
  const [view, setView] = useLocalStorage<View>("pley.live.view", "guide");
  const [stored, setStored] = useLocalStorage("pley.live.category", "all");
  const [search, setSearch] = useState("");
  const [picking, setPicking] = useState(false);
  const [details, setDetails] = useState<{ channel: Channel; program: Program } | null>(null);
  const chosen = parseCategory(stored);
  // A group gone, or nothing marked any more: every channel.
  const category: LiveCategory = groups.data && !stillThere(chosen, groups.data) ? { kind: "all" } : chosen;
  const select = (c: LiveCategory) => {
    setStored(categoryKey(c));
    setSearch("");
    setPicking(false);
  };
  // Details follow the channel as it changes (a favourite set, say).
  const detailChannel = useQuery({
    ...liveQueries.channel(details?.channel.id ?? 0),
    enabled: details != null,
    initialData: details?.channel,
    initialDataUpdatedAt: 0,
  }).data;
  const name = categoryName(category, groups.data?.groups);
  const query = search.trim().toLowerCase();
  const searching = query.length > 0;
  const foundGroups = searching ? (groups.data?.groups ?? []).filter((g) => g.name.toLowerCase().includes(query)).slice(0, 12) : [];
  const total =
    category.kind === "all"
      ? groups.data?.total
      : category.kind === "favorites"
        ? groups.data?.favorites
        : category.kind === "recent"
          ? groups.data?.recent
          : groups.data?.groups.find((g) => g.name === category.group)?.count;

  if (summary.data && !summary.data.allowed) {
    return (
      <Page>
        <EmptyState icon={<Tv2 />} title={t("notOpen")} description={t("page.notOpenHint")} />
      </Page>
    );
  }
  if (summary.isError || groups.isError) {
    const failed = summary.isError ? summary : groups;
    return (
      <Page>
        <EmptyState
          icon={<Tv2 />}
          title={t("didntLoad")}
          description={errorMessage(failed.error)}
          action={
            <Button icon={<RefreshCw className="size-4" />} onClick={() => void failed.refetch()}>
              {t("common:actions.tryAgain")}
            </Button>
          }
        />
      </Page>
    );
  }
  if (summary.data && !summary.data.available) {
    return (
      <Page>
        <EmptyState
          icon={<Tv2 />}
          title={user.is_admin ? t("page.setUpTitle") : t("page.noChannelsTitle")}
          description={user.is_admin ? t("page.setUpHint") : t("page.noChannelsHint")}
          action={
            user.is_admin ? (
              <Link to="/settings/livetv" className={buttonClass("primary")}>
                <Settings2 className="size-4" /> {t("page.setUp")}
              </Link>
            ) : undefined
          }
        />
      </Page>
    );
  }

  const rail = <CategoryRail groups={groups.data} selected={category} onSelect={select} className="h-full" />;
  const onDetails = (channel: Channel, program: Program) => setDetails({ channel, program });

  return (
    <Page
      actions={
        <div className="flex items-center gap-2">
          <div role="tablist" aria-label={t("page.view")} className="flex rounded-lg border border-border bg-surface p-0.5">
            {(
              [
                ["guide", t("page.guide"), CalendarDays],
                ["channels", t("channels"), LayoutGrid],
              ] as const
            ).map(([key, label, Icon]) => (
              <button
                key={key}
                role="tab"
                aria-selected={view === key}
                aria-label={label}
                onClick={() => setView(key)}
                className={cn(
                  "flex h-8 items-center gap-1.5 rounded-md px-3 text-sm font-medium transition-colors",
                  view === key ? "bg-surface-3 text-fg shadow-sm" : "text-muted hover:text-fg",
                )}
              >
                <Icon className="size-4" />
                <span className="hidden sm:inline">{label}</span>
              </button>
            ))}
          </div>
          {user.is_admin ? (
            <Link to="/settings/livetv" className={buttonClass("ghost", "sm", "size-9 px-0")} aria-label={t("settingsTitle")} title={t("settingsTitle")}>
              <Settings2 className="size-4" />
            </Link>
          ) : null}
        </div>
      }
    >
      <div className="flex gap-4 lg:gap-5">
        <aside className="hidden h-[calc(100dvh-9.75rem)] min-h-[22rem] w-64 shrink-0 overflow-hidden rounded-lg border border-border bg-surface lg:block xl:w-72">
          {rail}
        </aside>

        <div className="flex min-w-0 flex-1 flex-col">
          <div className="mb-3 flex flex-col gap-2 sm:flex-row sm:items-center">
            {/* The category, and on narrow screens the way to change it. */}
            <button
              type="button"
              onClick={() => setPicking(true)}
              className="flex h-9 min-w-0 items-center gap-2 rounded-md border border-border bg-surface px-3 text-left text-sm font-medium transition-colors hover:border-border-strong lg:hidden"
            >
              <span className="truncate">{searching ? t("page.resultsFor", { search: search.trim() }) : name}</span>
              {total !== undefined && !searching ? <span className="text-xs tabular-nums text-subtle">{total.toLocaleString(currentLocale())}</span> : null}
              <ChevronDown className="ml-auto size-4 shrink-0 text-subtle" />
            </button>
            <h2 className="hidden min-w-0 flex-1 items-baseline gap-2 lg:flex">
              {searching ? (
                <span className="truncate text-lg font-semibold tracking-tight">{t("page.resultsFor", { search: search.trim() })}</span>
              ) : (
                <>
                  <span className="truncate text-lg font-semibold tracking-tight">{name}</span>
                  {total !== undefined ? <span className="text-sm tabular-nums text-subtle">{t("page.channelCount", { count: total })}</span> : null}
                </>
              )}
            </h2>
            <div className="relative sm:ml-auto sm:w-80">
              <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-subtle" />
              <TextInput
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") setSearch("");
                }}
                placeholder={t("page.searchPlaceholder")}
                aria-label={t("page.searchLabel")}
                className="h-9 pl-9 pr-9"
              />
              {search ? (
                <button
                  type="button"
                  onClick={() => setSearch("")}
                  aria-label={t("common:actions.clear")}
                  className="absolute right-2 top-1/2 flex size-6 -translate-y-1/2 items-center justify-center rounded-full text-subtle hover:bg-white/10 hover:text-fg"
                >
                  <X className="size-3.5" />
                </button>
              ) : null}
            </div>
          </div>

          {searching && foundGroups.length ? (
            <div className="mb-3 flex flex-wrap items-center gap-1.5" role="group" aria-label={t("page.groupsFound")}>
              <span className="mr-1 text-xs font-semibold uppercase tracking-wider text-subtle">{t("page.groups")}</span>
              {foundGroups.map((g) => (
                <button
                  key={g.name}
                  type="button"
                  onClick={() => select({ kind: "group", group: g.name })}
                  className="flex h-7 items-center gap-1.5 rounded-full border border-border px-3 text-sm text-muted transition-colors hover:border-accent/50 hover:text-fg"
                >
                  {g.region ? <span className="text-xs font-semibold text-subtle">{g.region}</span> : null}
                  {g.label}
                  <span className="text-xs tabular-nums text-subtle">{g.count.toLocaleString(currentLocale())}</span>
                </button>
              ))}
            </div>
          ) : null}
          {/* Search looks among every channel, whatever group is open. */}
          <Results category={searching ? { kind: "all" } : category} search={search} view={view} now={now} onDetails={onDetails} />
        </div>
      </div>

      <Modal open={picking} onClose={() => setPicking(false)} title={t("categories.label")} size="md">
        <div className="-mx-6 -my-2 h-[min(70dvh,36rem)]">{rail}</div>
      </Modal>
      <ProgramModal channel={details ? (detailChannel ?? details.channel) : null} program={details?.program ?? null} category={category} onClose={() => setDetails(null)} />
    </Page>
  );
}

/** The channels of the category: the guide or the cards, or why there are none. */
function Results({
  category,
  search,
  view,
  now,
  onDetails,
}: {
  category: LiveCategory;
  search: string;
  view: View;
  now: number;
  onDetails: (channel: Channel, program: Program) => void;
}) {
  const { t } = useTranslation(["livetv", "common"]);
  const first = useQuery(liveQueries.page(category, search, 0));
  const height = "h-[calc(100dvh-13rem)] min-h-[22rem] sm:h-[calc(100dvh-12.5rem)]";
  if (first.isError) {
    return (
      <EmptyState
        icon={<Tv2 />}
        title={t("page.channelsDidntLoad")}
        description={errorMessage(first.error)}
        action={
          <Button icon={<RefreshCw className="size-4" />} onClick={() => void first.refetch()}>
            {t("common:actions.tryAgain")}
          </Button>
        }
      />
    );
  }
  if (!first.data) return <Skeleton className={cn(height, "w-full rounded-lg")} />;
  if (first.data.total === 0) {
    return (
      <EmptyState
        icon={<Search />}
        title={search ? t("page.noMatch") : t("page.empty")}
        description={search ? t("page.noMatchHint", { search: search.trim() }) : t("page.emptyHint")}
      />
    );
  }
  return view === "guide" ? (
    <div className={height}>
      <GuideGrid category={category} search={search} now={now} onDetails={onDetails} />
    </div>
  ) : (
    <div className={cn(height, "overflow-hidden rounded-lg border border-border bg-surface/40")}>
      <ChannelGrid category={category} search={search} now={now} onDetails={onDetails} />
    </div>
  );
}

function Page({ children, actions }: { children: React.ReactNode; actions?: React.ReactNode }) {
  const { t } = useTranslation("livetv");
  return (
    <div className="px-4 pb-6 pt-6 sm:px-8">
      <div className="mb-5 flex items-center gap-3">
        <div className="flex size-10 items-center justify-center rounded-lg bg-surface-2 text-accent">
          <Tv2 className="size-5" />
        </div>
        <h1 className="flex-1 text-2xl font-bold tracking-tight">{t("liveTv")}</h1>
        {actions}
      </div>
      {children}
    </div>
  );
}
