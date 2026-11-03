import { Link, useSearchParams } from "react-router";
import { Search } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useSearch } from "@/api/hooks";
import { programProgress, type Channel } from "@/api/livetv";
import { mayRequest, requestProblem, uniqueTitles, useDiscoverSearch, useFillPages, useRequestSummary } from "@/api/requests";
import type { ItemKind, ItemSummary } from "@/api/types";
import { aspectFor, CardSkeleton, MediaCard } from "@/components/cards";
import { PersonCard } from "@/components/PersonCard";
import { Button, EmptyState, SectionTitle, Spinner } from "@/components/ui";
import { useDocumentTitle, useElementWidth } from "@/lib/hooks";
import { ChannelLogo, liveUrl, useNow } from "@/pages/livetv/common";
import { problemWords } from "@/lib/requests";
import { discoverUrl } from "@/lib/urls";
import { ScrollRowSkeleton, TitleCards } from "./requests/shared";

const GROUPS: { kind: ItemKind; title: string }[] = [
  { kind: "movie", title: "Movies" },
  { kind: "show", title: "Shows" },
  { kind: "season", title: "Seasons" },
  { kind: "episode", title: "Episodes" },
  { kind: "artist", title: "Artists" },
  { kind: "album", title: "Albums" },
  { kind: "track", title: "Tracks" },
];

function ResultGroup({ title, items }: { title: string; items: ItemSummary[] }) {
  const [ref, width] = useElementWidth<HTMLDivElement>();
  const aspect = items[0] ? aspectFor(items[0]) : "poster";
  const target = aspect === "thumb" ? 260 : aspect === "square" ? 160 : 150;
  const gap = 18;
  const cols = width > 0 ? Math.max(2, Math.floor((width + gap) / (target + gap))) : 4;
  const colWidth = width > 0 ? (width - gap * (cols - 1)) / cols : target;
  return (
    <section>
      <SectionTitle>
        {title} <span className="ml-1 text-sm font-normal text-subtle">{items.length}</span>
      </SectionTitle>
      <div ref={ref} className="grid gap-x-[18px] gap-y-6" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}>
        {items.map((item) => (
          <MediaCard key={item.id} item={item} aspect={aspect} width={colWidth} playable={item.kind !== "track"} />
        ))}
      </div>
    </section>
  );
}

/** Live TV channels found: logo, number and name, what's on with how far it is. */
function ChannelResults({ channels }: { channels: Channel[] }) {
  const now = useNow();
  return (
    <section>
      <SectionTitle>
        Live TV <span className="ml-1 text-sm font-normal text-subtle">{channels.length}</span>
      </SectionTitle>
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-3">
        {channels.map((c) => {
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
    </section>
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
  const search = useDiscoverSearch(q, { missing: true });
  useFillPages(search, TMDB_TITLES, 3);
  const titles = uniqueTitles(search.data?.pages).slice(0, TMDB_TITLES);

  if (search.isPending) return <ScrollRowSkeleton title="Discover" inset />;
  if (titles.length) return <TitleCards titles={titles} title="Discover" link={discoverUrl(q)} inset />;
  // All TMDB has is in a library already: nothing to add under the libraries' results.
  if (!search.isError && !alone) return null;
  const problem = search.isError ? requestProblem(search.error) : null;
  const words = problem ? problemWords(problem, { manage }) : null;
  return (
    <section>
      <SectionTitle>Discover</SectionTitle>
      {search.isError ? (
        <div role="alert" className="flex flex-wrap items-center gap-3 text-sm">
          <span className="text-danger">{words ? words.title : `Couldn't search TMDB: ${errorMessage(search.error)}`}</span>
          {!words || words.retry ? (
            <Button size="sm" loading={search.isFetching} onClick={() => void search.refetch()}>
              Retry
            </Button>
          ) : null}
        </div>
      ) : (
        <p className="text-sm text-muted">Nothing there either. Try a different spelling or a shorter query.</p>
      )}
    </section>
  );
}

export function SearchPage() {
  const [params] = useSearchParams();
  const q = params.get("q") ?? "";
  useDocumentTitle(q ? `Search: ${q}` : "Search");
  const search = useSearch(q);
  const summary = useRequestSummary(q.trim() !== "").data;

  if (!q.trim()) {
    return (
      <EmptyState
        className="min-h-[60vh]"
        icon={<Search />}
        title="Search your libraries"
        description="Find movies, shows, episodes, people, artists, albums, tracks and Live TV channels. Press / anywhere to jump to the search box."
      />
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
          title="Couldn't search your libraries"
          description={errorMessage(search.error)}
          action={<Button onClick={() => void search.refetch()}>Retry</Button>}
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
            <h1 className="text-xl font-semibold tracking-tight">Results for “{q}”</h1>
            <p className="mt-1 text-sm text-muted">Nothing in your libraries matches.</p>
          </div>
          {tmdb(true)}
        </div>
      );
    }
    return (
      <EmptyState
        className="min-h-[60vh]"
        icon={<Search />}
        title={`No results for “${q}”`}
        description="Try a different spelling or a shorter query."
      />
    );
  }

  return (
    <div className="space-y-10 px-4 py-8 sm:px-8">
      <div className="flex items-center gap-3">
        <h1 className="text-xl font-semibold tracking-tight">
          Results for “{q}”
        </h1>
        {search.isFetching ? <Spinner className="size-4" /> : null}
      </div>
      {/* The libraries' titles, then Live TV, then people (the server puts the closest first in each). */}
      {GROUPS.map(({ kind, title }) => {
        const items = results.filter((r) => r.kind === kind);
        return items.length ? <ResultGroup key={kind} title={title} items={items} /> : null;
      })}
      {channels.length ? <ChannelResults channels={channels} /> : null}
      {people.length ? (
        <section>
          <SectionTitle>
            People <span className="ml-1 text-sm font-normal text-subtle">{people.length}</span>
          </SectionTitle>
          <div className="flex flex-wrap gap-x-4 gap-y-5">
            {people.map((p) => (
              <PersonCard key={p.id} person={p} width={112} />
            ))}
          </div>
        </section>
      ) : null}
      {tmdb()}
    </div>
  );
}
