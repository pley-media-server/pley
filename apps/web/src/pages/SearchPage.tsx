import { useId, useRef, useState, type ReactNode } from "react";
import { Link, useSearchParams } from "react-router";
import { useTranslation } from "react-i18next";
import { ChevronDown, Search } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useSearch } from "@/api/hooks";
import { programProgress, type Channel } from "@/api/livetv";
import { mayRequest, requestProblem, uniqueTitles, useDiscoverSearch, useFillPages, useRequestSummary } from "@/api/requests";
import type { ItemKind, ItemSummary, PersonSummary } from "@/api/types";
import { aspectFor, CardSkeleton, MediaCard } from "@/components/cards";
import { PersonCard } from "@/components/PersonCard";
import { Button, EmptyState, SectionTitle, Spinner } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { useDocumentTitle, useElementWidth } from "@/lib/hooks";
import { ChannelLogo, liveUrl, useNow } from "@/pages/livetv/common";
import { problemWords } from "@/lib/requests";
import { discoverUrl } from "@/lib/urls";
import { ScrollRowSkeleton, TitleCards } from "./requests/shared";

/** The groups of results, in order, each titled in `search.groups`. */
const GROUPS = ["movie", "show", "season", "episode", "artist", "album", "track"] as const satisfies readonly ItemKind[];

/** Rows of a group shown until it's opened: the closest matches come first. */
const SHOWN_ROWS = 2;

/** Columns of about `target` px, `gap` apart, that fit in `width` (at least `min`; `fallback` before it's measured). */
function columnsIn(width: number, target: number, gap: number, min = 1, fallback = 4): number {
  return width > 0 ? Math.max(min, Math.floor((width + gap) / (target + gap))) : fallback;
}

/**
 * A group of results: its title and count, its first two rows, and "Show
 * all" for the rest. Closing a long group brings its title back into view,
 * rather than leaving the page far below it.
 */
function ResultSection<T>({
  title,
  items,
  columns,
  children,
}: {
  title: string;
  items: T[];
  /** How many results a row holds. */
  columns: number;
  children: (shown: T[]) => ReactNode;
}) {
  const { t } = useTranslation("pages");
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLElement>(null);
  const id = useId();
  const limit = columns * SHOWN_ROWS;
  const more = items.length > limit;
  const toggle = () => {
    const top = ref.current?.getBoundingClientRect().top ?? 0;
    if (open && top < 0) ref.current?.scrollIntoView({ block: "start" });
    setOpen(!open);
  };
  return (
    <section ref={ref} className="scroll-mt-20" aria-labelledby={`${id}-title`}>
      <SectionTitle
        action={
          more ? (
            <button
              type="button"
              aria-expanded={open}
              aria-controls={`${id}-results`}
              aria-describedby={`${id}-title`}
              onClick={toggle}
              className="inline-flex h-8 shrink-0 items-center gap-1 rounded-full px-3 text-sm font-medium text-muted transition-colors hover:bg-white/[0.06] hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            >
              {open ? t("search.showFewer") : t("search.showAll", { n: items.length })}
              <ChevronDown className={cn("size-4 transition-transform duration-200", open && "rotate-180")} aria-hidden />
            </button>
          ) : null
        }
      >
        <span id={`${id}-title`}>
          {title} <span className="ml-1 text-sm font-normal text-subtle">{items.length.toLocaleString(currentLocale())}</span>
        </span>
      </SectionTitle>
      <div id={`${id}-results`}>{children(open || !more ? items : items.slice(0, limit))}</div>
    </section>
  );
}

function ResultGroup({ title, items }: { title: string; items: ItemSummary[] }) {
  const [ref, width] = useElementWidth<HTMLDivElement>();
  const aspect = items[0] ? aspectFor(items[0]) : "poster";
  const target = aspect === "thumb" ? 260 : aspect === "square" ? 160 : 150;
  const gap = 18;
  const cols = columnsIn(width, target, gap, 2);
  const colWidth = width > 0 ? (width - gap * (cols - 1)) / cols : target;
  return (
    <ResultSection title={title} items={items} columns={cols}>
      {(shown) => (
        <div ref={ref} className="grid gap-x-[18px] gap-y-6" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}>
          {shown.map((item) => (
            <MediaCard key={item.id} item={item} aspect={aspect} width={colWidth} playable={item.kind !== "track"} />
          ))}
        </div>
      )}
    </ResultSection>
  );
}

/** People found: round photos, as many to a row as fit. */
function PeopleResults({ people }: { people: PersonSummary[] }) {
  const { t } = useTranslation("pages");
  const [ref, width] = useElementWidth<HTMLDivElement>();
  const cols = columnsIn(width, 112, 16, 2, 6);
  return (
    <ResultSection title={t("search.people")} items={people} columns={cols}>
      {(shown) => (
        <div ref={ref} className="grid gap-x-4 gap-y-5" style={{ gridTemplateColumns: `repeat(${cols}, 112px)` }}>
          {shown.map((p) => (
            <PersonCard key={p.id} person={p} width={112} />
          ))}
        </div>
      )}
    </ResultSection>
  );
}

/** Live TV channels found: logo, number and name, what's on with how far it is. */
function ChannelResults({ channels }: { channels: Channel[] }) {
  const { t } = useTranslation("pages");
  const now = useNow();
  const [ref, width] = useElementWidth<HTMLUListElement>();
  const cols = columnsIn(width, 256, 12, 1, 3);
  return (
    <ResultSection title={t("search.liveTv")} items={channels} columns={cols}>
      {(shown) => (
        <ul ref={ref} className="grid gap-3" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}>
          {shown.map((c) => {
            const on = c.now && c.now.start <= now && c.now.stop > now ? c.now : null;
            return (
              <li key={c.id}>
                <Link
                  to={liveUrl(c)}
                  className="flex items-center gap-3 rounded-lg border border-border bg-surface p-2.5 transition-colors hover:border-border-strong hover:bg-surface-2"
                >
                  <ChannelLogo channel={c} width={96} showName={false} className="h-11 w-[4.5rem] shrink-0 rounded-md bg-surface-3" />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline gap-1.5">
                      {c.number ? <span className="shrink-0 text-xs tabular-nums text-subtle">{c.number}</span> : null}
                      <span className="truncate text-sm font-medium">{c.name}</span>
                    </div>
                    {on ? (
                      <>
                        <div className="truncate text-xs text-muted">{on.title}</div>
                        <div className="mt-1 h-0.5 overflow-hidden rounded-full bg-white/10">
                          <div className="h-full bg-accent" style={{ width: `${programProgress(on, now) * 100}%` }} />
                        </div>
                      </>
                    ) : (
                      <div className="truncate text-xs text-subtle">{c.group}</div>
                    )}
                  </div>
                </Link>
              </li>
            );
          })}
        </ul>
      )}
    </ResultSection>
  );
}

/** How many of TMDB's titles the search shows (more are on Discover). */
const TMDB_TITLES = 20;

/**
 * What TMDB has for the words that the libraries don't, after their
 * results: asked for in parallel with them, never holding them back.
 * `alone`: the libraries had nothing, so it says when TMDB has nothing too.
 */
function OnTmdb({ q, manage, alone }: { q: string; manage: boolean; alone: boolean }) {
  const { t } = useTranslation(["pages", "common"]);
  const search = useDiscoverSearch(q, { missing: true });
  useFillPages(search, TMDB_TITLES, 3);
  const titles = uniqueTitles(search.data?.pages).slice(0, TMDB_TITLES);

  if (search.isPending) return <ScrollRowSkeleton title={t("search.discover")} inset />;
  if (titles.length) return <TitleCards titles={titles} title={t("search.discover")} link={discoverUrl(q)} inset />;
  // All TMDB has is in a library already: nothing to add under the libraries' results.
  if (!search.isError && !alone) return null;
  const problem = search.isError ? requestProblem(search.error) : null;
  const words = problem ? problemWords(problem, { manage }) : null;
  return (
    <section>
      <SectionTitle>{t("search.discover")}</SectionTitle>
      {search.isError ? (
        <div role="alert" className="flex flex-wrap items-center gap-3 text-sm">
          <span className="text-danger">{words ? words.title : t("search.cantSearchTmdb", { error: errorMessage(search.error) })}</span>
          {!words || words.retry ? (
            <Button size="sm" loading={search.isFetching} onClick={() => void search.refetch()}>
              {t("common:actions.retry")}
            </Button>
          ) : null}
        </div>
      ) : (
        <p className="text-sm text-muted">{t("search.nothingThere")}</p>
      )}
    </section>
  );
}

export function SearchPage() {
  const { t } = useTranslation(["pages", "common"]);
  const [params] = useSearchParams();
  const q = params.get("q") ?? "";
  useDocumentTitle(q ? t("search.documentTitleQuery", { q }) : t("search.documentTitle"));
  const search = useSearch(q);
  const summary = useRequestSummary(q.trim() !== "").data;

  if (!q.trim()) {
    return (
      <EmptyState className="min-h-[60vh]" icon={<Search />} title={t("search.emptyTitle")} description={t("search.emptyHint")} />
    );
  }

  // Where this account may ask for titles: what the libraries lack, on TMDB.
  const tmdb = (alone = false) => (summary && mayRequest(summary) ? <OnTmdb q={q.trim()} manage={summary.manage} alone={alone} /> : null);

  if (search.isPending) {
    return (
      <div className="space-y-10 px-4 py-8 sm:px-8">
        <div className="grid gap-x-[18px] gap-y-6" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))" }}>
          {Array.from({ length: 12 }, (_, i) => (
            <CardSkeleton key={i} aspect="poster" />
          ))}
        </div>
        {tmdb()}
      </div>
    );
  }

  if (search.isError) {
    return (
      <div className="px-4 pb-8 sm:px-8">
        <EmptyState
          className="min-h-[40vh]"
          title={t("search.cantSearch")}
          description={errorMessage(search.error)}
          action={<Button onClick={() => void search.refetch()}>{t("common:actions.retry")}</Button>}
        />
        {tmdb()}
      </div>
    );
  }

  const results = search.data?.items ?? [];
  const people = search.data?.people ?? [];
  const channels = search.data?.channels ?? [];
  if (!results.length && !people.length && !channels.length) {
    // TMDB's titles still show, under a line saying the libraries have none.
    if (mayRequest(summary)) {
      return (
        <div className="space-y-8 px-4 py-8 sm:px-8">
          <div>
            <h1 className="text-xl font-semibold tracking-tight">{t("search.resultsFor", { q })}</h1>
            <p className="mt-1 text-sm text-muted">{t("search.nothingInLibraries")}</p>
          </div>
          {tmdb(true)}
        </div>
      );
    }
    return (
      <EmptyState
        className="min-h-[60vh]"
        icon={<Search />}
        title={t("search.noResults", { q })}
        description={t("search.differentSpelling")}
      />
    );
  }

  return (
    <div className="space-y-10 px-4 py-8 sm:px-8">
      <div className="flex items-center gap-3">
        <h1 className="text-xl font-semibold tracking-tight">{t("search.resultsFor", { q })}</h1>
        {search.isFetching ? <Spinner className="size-4" /> : null}
      </div>
      {/* The libraries' titles, then Live TV, then people (the server puts the closest first in each). */}
      {GROUPS.map((kind) => {
        const items = results.filter((r) => r.kind === kind);
        // Keyed by the query too: a new search starts with every group closed.
        // Each group's key is its own: siblings sharing one leave an old group on the page.
        return items.length ? <ResultGroup key={`${q}:${kind}`} title={t(`search.groups.${kind}`)} items={items} /> : null;
      })}
      {channels.length ? <ChannelResults key={`${q}:channels`} channels={channels} /> : null}
      {people.length ? <PeopleResults key={`${q}:people`} people={people} /> : null}
      {tmdb()}
    </div>
  );
}
