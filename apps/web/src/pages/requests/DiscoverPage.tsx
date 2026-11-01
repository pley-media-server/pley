import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { Ban, ChevronLeft, Inbox, Search, SearchX, ShieldCheck, X } from "lucide-react";
import { isApiError } from "@/api/client";
import {
  hasRequests,
  requestFilter,
  requestProblem,
  SEARCH_MAX,
  searchText,
  uniqueTitles,
  useDiscoverList,
  useDiscoverSearch,
  useDiscoverTitle,
  useFillPages,
  useRequests,
  useRequestSummary,
  type DiscoverList,
  type DiscoverTitle,
  type MediaRequest,
  type RequestSummary,
} from "@/api/requests";
import { useAuth } from "@/auth";
import { Artwork } from "@/components/Artwork";
import { ASPECT_CLASS } from "@/components/cards";
import { HubRowSkeleton } from "@/components/HubRow";
import { buttonClass, EmptyState, Skeleton, Spinner } from "@/components/ui";
import { cn } from "@/lib/cn";
import { useDebouncedValue, useDocumentTitle, useOnChange } from "@/lib/hooks";
import { kindLabel, problemWords, requestsBadge, seasonsLabel, stageWords } from "@/lib/requests";
import { discoverUrl, requestsUrl, titleUrl } from "@/lib/urls";
import { MainActions } from "./TitlePage";
import { ProblemState, RequestsUnavailable, ScrollRow, StagePill, TitleGrid, TitleGridSkeleton, TitleRow, TmdbCredit } from "./shared";

const LISTS: { list: DiscoverList; title: string }[] = [
  { list: "trending", title: "Trending this week" },
  { list: "movies", title: "Popular movies" },
  { list: "shows", title: "Popular shows" },
  { list: "upcoming", title: "Coming to cinemas" },
  { list: "anime", title: "Popular anime" },
];

/** A grid loads on past pages that came short until it holds this many. */
const GRID_TITLES = 24;

function listOf(value: string | null): (typeof LISTS)[number] | null {
  return LISTS.find((l) => l.list === value) ?? null;
}

/** TMDB's search, kept in the address (`?q=`) as one types. */
function SearchBox({ q }: { q: string }) {
  const [params, setParams] = useSearchParams();
  const [value, setValue] = useState(q);
  const debounced = useDebouncedValue(value, 300);
  const inputRef = useRef<HTMLInputElement>(null);

  // Another query arriving (a link from the libraries' search, Discover in
  // the menu) goes in the box; the one just typed is there already.
  useOnChange(q, (next) => {
    if (next.trim() !== debounced.trim()) setValue(next);
  });

  useEffect(() => {
    const next = debounced.trim();
    if (next === q.trim()) return;
    const p = new URLSearchParams(params);
    if (next) p.set("q", next);
    else p.delete("q");
    p.delete("list");
    setParams(p, { replace: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [debounced]);

  return (
    <div className="relative max-w-xl" role="search">
      <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-subtle" />
      <input
        ref={inputRef}
        type="search"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") setValue("");
        }}
        maxLength={SEARCH_MAX}
        placeholder="Search TMDB for movies and shows"
        aria-label="Search for movies and shows to ask for"
        className="h-11 w-full rounded-full border border-border bg-surface-2 pl-9 pr-9 text-sm text-fg placeholder:text-subtle transition-colors focus:border-accent/70 focus:bg-surface-3 focus:outline-none [&::-webkit-search-cancel-button]:hidden"
      />
      {value ? (
        <button
          type="button"
          aria-label="Clear search"
          onClick={() => {
            setValue("");
            inputRef.current?.focus();
          }}
          className="absolute right-3 top-1/2 -translate-y-1/2 rounded-full p-1 text-subtle hover:text-fg"
        >
          <X className="size-3.5" />
        </button>
      ) : null}
    </div>
  );
}

function SearchResults({ q, manage }: { q: string; manage: boolean }) {
  const search = useDiscoverSearch(q);
  useFillPages(search, GRID_TITLES);
  const titles = uniqueTitles(search.data?.pages);

  if (search.isPending) return <TitleGridSkeleton />;
  if (search.isError && !search.data) {
    return (
      <ProblemState
        className="py-16"
        error={search.error}
        manage={manage}
        title="Couldn't search TMDB"
        retrying={search.isFetching}
        onRetry={() => void search.refetch()}
      />
    );
  }
  if (!titles.length && !search.hasNextPage) {
    return <EmptyState icon={<SearchX />} title={`Nothing on TMDB for “${q}”`} description="Try another spelling, or the title in its own language." />;
  }
  return (
    <section>
      <div className="mb-3 flex items-center gap-3">
        <h2 className="text-lg font-semibold tracking-tight">Results for “{q}”</h2>
        {search.isFetching ? <Spinner className="size-4" /> : null}
      </div>
      <TitleGrid
        titles={titles}
        more={{ next: search.hasNextPage, loading: search.isFetchingNextPage, load: () => void search.fetchNextPage() }}
      />
    </section>
  );
}

/** One of TMDB's lists whole; `ageLimit`: titles above this profile's age limit are left out of it. */
function ListGrid({ list, title, manage, ageLimit }: { list: DiscoverList; title: string; manage: boolean; ageLimit: boolean }) {
  const query = useDiscoverList(list);
  useFillPages(query, GRID_TITLES);
  const titles = uniqueTitles(query.data?.pages);
  return (
    <section>
      <div className="mb-3 flex items-center gap-2">
        <Link to={discoverUrl()} aria-label="Back to Discover" className="-ml-1.5 rounded-full p-1 text-muted transition-colors hover:bg-white/10 hover:text-fg">
          <ChevronLeft className="size-5" />
        </Link>
        <h2 className="text-lg font-semibold tracking-tight">{title}</h2>
      </div>
      {query.isPending ? (
        <TitleGridSkeleton />
      ) : query.isError && !query.data ? (
        <ProblemState className="py-16" error={query.error} manage={manage} retrying={query.isFetching} onRetry={() => void query.refetch()} />
      ) : !titles.length && !query.hasNextPage && !query.isFetching ? (
        <EmptyState
          icon={ageLimit ? <ShieldCheck /> : <SearchX />}
          title="Nothing here right now"
          description={ageLimit ? "What TMDB lists here is above this profile's age limit." : "TMDB has nothing in this list today."}
          action={
            <Link to={discoverUrl()} className={buttonClass("outline")}>
              Back to Discover
            </Link>
          }
        />
      ) : (
        <TitleGrid titles={titles} more={{ next: query.hasNextPage, loading: query.isFetchingNextPage, load: () => void query.fetchNextPage() }} />
      )}
    </section>
  );
}

function HeroSkeleton() {
  return (
    <div className="px-4 sm:px-8" aria-hidden>
      <Skeleton className="min-h-[340px] w-full rounded-xl sm:min-h-[420px] lg:min-h-[480px]" />
    </div>
  );
}

/**
 * The first trending title, large: its backdrop, a line of what it is,
 * and the one thing to do about it (a show's seasons are picked on its
 * page).
 */
function Hero({ summary }: { summary: RequestSummary }) {
  const navigate = useNavigate();
  const trending = useDiscoverList("trending");
  // The first title left (an age limit may leave a page empty).
  useFillPages(trending, 1);
  const first: DiscoverTitle | undefined = uniqueTitles(trending.data?.pages)[0];
  const detail = useDiscoverTitle(first?.kind ?? "movie", first?.tmdb_id ?? 0, !!first);

  if (!first) return trending.isPending || trending.isFetchingNextPage ? <HeroSkeleton /> : null;
  const to = titleUrl(first.kind, first.tmdb_id);
  return (
    <section aria-label="Trending now" className="px-4 sm:px-8">
      <div className="relative isolate overflow-hidden rounded-xl bg-surface-2 ring-1 ring-white/5">
        {/* Placed by a box of its own: the picture's is positioned already (relative). */}
        <div className="absolute inset-0 -z-10">
          <Artwork
            candidates={first.backdrop_url ? [first.backdrop_url] : []}
            width={1280}
            alt=""
            title={first.title}
            bare
            eager
            className="size-full"
            imgClassName="object-cover object-[center_25%]"
          />
        </div>
        <div className="absolute inset-0 -z-10 bg-linear-to-t from-bg via-bg/60 to-transparent" />
        <div className="absolute inset-0 -z-10 bg-linear-to-r from-bg/90 via-bg/40 to-transparent" />
        <div className="flex min-h-[340px] flex-col justify-end gap-3 p-5 sm:min-h-[420px] sm:p-8 lg:min-h-[480px] lg:max-w-[60%]">
          <p className="text-xs font-semibold uppercase tracking-wider text-accent">Trending this week</p>
          <h2 className="text-3xl font-bold leading-tight tracking-tight text-balance sm:text-4xl">
            <Link to={to} className="transition-colors hover:text-accent">
              {first.title}
            </Link>
          </h2>
          <p className="text-sm text-muted">{[first.year, kindLabel(first.kind)].filter(Boolean).join(" · ")}</p>
          {first.overview ? <p className="line-clamp-2 max-w-2xl text-sm text-fg/85">{first.overview}</p> : null}
          <div className="pt-1">
            {detail.data ? (
              <MainActions title={detail.data} summary={summary} size="md" onPick={() => void navigate(`${to}?pick=1`)} />
            ) : (
              <Link to={to} className={buttonClass("secondary")}>
                More about it
              </Link>
            )}
          </div>
        </div>
      </div>
    </section>
  );
}

/** A request as a poster in a row: its title, what of it was asked for, and its stage. */
function RequestPoster({ request, width, manage }: { request: MediaRequest; width: number; manage: boolean }) {
  const to = titleUrl(request.kind, request.tmdb_id);
  const subtitle = seasonsLabel(request.seasons) || [request.year, kindLabel(request.kind)].filter(Boolean).join(" · ");
  return (
    <div className="group/card min-w-0" style={{ width }}>
      <Link
        to={to}
        aria-label={`${request.title}, ${stageWords(request, manage).label}`}
        className="relative block overflow-hidden rounded-md bg-surface-2 outline-none ring-1 ring-white/5 transition duration-200 hover:ring-2 hover:ring-accent focus-visible:ring-2 focus-visible:ring-accent"
      >
        <Artwork
          candidates={request.poster_url ? [request.poster_url] : []}
          width={width}
          alt=""
          title={request.title}
          className={cn(ASPECT_CLASS.poster, "w-full")}
          imgClassName="transition-transform duration-300 motion-safe:group-hover/card:scale-[1.04]"
        />
      </Link>
      <div className="mt-2 space-y-1.5 px-0.5" aria-hidden>
        <div>
          <div className="truncate text-sm font-medium" title={request.title}>
            {request.title}
          </div>
          <div className="truncate text-xs text-muted">{subtitle}</div>
        </div>
        <StagePill request={request} manage={manage} line={false} compact />
      </div>
    </div>
  );
}

/** This account's requests and those it follows, newest first, for those who have any. */
function YourRequestsRow({ summary }: { summary: RequestSummary }) {
  const requests = useRequests(requestFilter({ mine: true }), { limit: 20 });
  const shown = requests.data?.pages.flatMap((p) => p.requests) ?? [];
  if (requests.isPending) return <HubRowSkeleton />;
  if (!shown.length) return null;
  return (
    <ScrollRow title="Your requests" link={requestsUrl} count={shown.length}>
      {(width) =>
        shown.map((r) => (
          <div key={r.id} className="shrink-0 snap-start">
            <RequestPoster request={r} width={width} manage={summary.manage} />
          </div>
        ))
      }
    </ScrollRow>
  );
}

/** Where requests are: the account's own, or everyone's for managers, with what waits there. */
function RequestsLink({ summary }: { summary: RequestSummary }) {
  const badge = requestsBadge(summary);
  return (
    <Link to={requestsUrl} className={buttonClass("outline", "sm")}>
      <Inbox className="size-4" />
      {summary.manage ? "Requests" : "Your requests"}
      {badge.count > 0 ? (
        <>
          {" "}
          <span
            className={cn(
              "min-w-5 rounded-full px-1.5 text-center text-[11px] font-bold leading-5",
              badge.danger ? "bg-danger text-white" : "bg-accent text-accent-fg",
            )}
          >
            {badge.count > 99 ? "99+" : badge.count} <span className="sr-only">waiting</span>
          </span>
        </>
      ) : null}
    </Link>
  );
}

/**
 * Discover's rows: the first trending title large, TMDB's lists, then this
 * account's requests. Where TMDB can't be used at all (it refused the key,
 * or isn't answering), one notice says so in place of every row.
 */
function DiscoverHome({ summary }: { summary: RequestSummary }) {
  const trending = useDiscoverList("trending");
  const problem = trending.data ? null : requestProblem(trending.error);
  const yours = summary.own > 0 ? <YourRequestsRow summary={summary} /> : null;
  if (problem?.kind === "tmdb_key" || problem?.kind === "tmdb_down") {
    return (
      <>
        <ProblemState className="py-12" problem={problem} manage={summary.manage} retrying={trending.isFetching} onRetry={() => void trending.refetch()} />
        {yours}
      </>
    );
  }
  return (
    <>
      <Hero summary={summary} />
      <div className="space-y-8">
        {LISTS.map((l) => (
          <TitleRow key={l.list} list={l.list} title={l.title} link={`/discover?list=${l.list}`} />
        ))}
        {yours}
      </div>
    </>
  );
}

function DiscoverSkeleton() {
  return (
    <div className="space-y-8 pb-16 pt-6 sm:pt-8" aria-busy>
      <div className="space-y-4 px-4 sm:px-8">
        <Skeleton className="h-8 w-40" />
        <Skeleton className="h-11 w-full max-w-xl rounded-full" />
      </div>
      <HeroSkeleton />
      <HubRowSkeleton />
    </div>
  );
}

/**
 * Discover (`/discover`): the first trending title large, then TMDB's
 * lists as rows that load as they come into view, and this account's
 * requests; one list whole (`?list=`), or what TMDB finds (`?q=`).
 */
export function DiscoverPage() {
  const me = useAuth();
  const [params] = useSearchParams();
  const q = params.get("q") ?? "";
  // What's searched for: a longer query (a link from the libraries' search) is cut to what the server takes.
  const searched = searchText(q);
  const list = listOf(params.get("list"));
  const summary = useRequestSummary();
  useDocumentTitle(searched ? `Discover: ${searched}` : "Discover");

  if (summary.isPending) return <DiscoverSkeleton />;
  const data = summary.data;
  if (!data) {
    // A server without requests answers 404 (an early one 403): as good as off.
    if (isApiError(summary.error, 404) || isApiError(summary.error, 403)) return <RequestsUnavailable reason="off" admin={me.is_admin} />;
    return (
      <ProblemState
        error={summary.error}
        manage={me.is_admin}
        title="Couldn't load Discover"
        retrying={summary.isFetching}
        onRetry={() => void summary.refetch()}
      />
    );
  }
  if (!data.enabled) return <RequestsUnavailable reason="off" admin={me.is_admin} />;
  if (data.sign_in_again === "requests") return <ProblemState problem={{ kind: "sign_in" }} manage={false} />;
  if (data.access === "off") {
    const words = problemWords({ kind: "no_access" }, { manage: false });
    return (
      <EmptyState
        className="min-h-[60vh]"
        icon={<Ban />}
        title={words.title}
        description={words.text}
        action={
          // What it asked for before stays its own to look at.
          hasRequests(data) ? (
            <Link to={requestsUrl} className={buttonClass("outline")}>
              See your requests
            </Link>
          ) : undefined
        }
      />
    );
  }

  return (
    <div className="space-y-8 pb-16 pt-6 sm:pt-8">
      <header className="space-y-4 px-4 sm:px-8">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h1 className="text-2xl font-bold tracking-tight">Discover</h1>
          {hasRequests(data) || data.manage ? <RequestsLink summary={data} /> : null}
        </div>
        <SearchBox q={q} />
      </header>

      {searched ? (
        <div className="px-4 sm:px-8">
          <SearchResults q={searched} manage={data.manage} />
        </div>
      ) : list ? (
        <div className="px-4 sm:px-8">
          <ListGrid list={list.list} title={list.title} manage={data.manage} ageLimit={me.max_age !== null} />
        </div>
      ) : (
        <DiscoverHome summary={data} />
      )}

      <TmdbCredit full className="px-4 sm:px-8" />
    </div>
  );
}
