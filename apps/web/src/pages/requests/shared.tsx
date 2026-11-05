import { memo, useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { Link, useLocation, useNavigate } from "react-router";
import { Ban, Check, CircleCheck, Clock, CloudOff, Inbox, KeyRound, LogIn } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useLogout } from "@/api/hooks";
import {
  requestProblem,
  useDiscoverList,
  useFillPages,
  uniqueTitles,
  type DiscoverList,
  type DiscoverTitle,
  type MediaRequest,
  type RequestProblem,
} from "@/api/requests";
import { useOptionalAudioPlayer } from "@/audio/AudioPlayer";
import { Artwork } from "@/components/Artwork";
import { ASPECT_CLASS, CardSkeleton } from "@/components/cards";
import { ArrowButton, HubRowSkeleton, useFittedRow } from "@/components/HubRow";
import { TMDB_KEY_SETTING } from "@/components/MetadataNotice";
import { Button, buttonClass, EmptyState } from "@/components/ui";
import { cn } from "@/lib/cn";
import { smoothScroll, useElementWidth } from "@/lib/hooks";
import { kindLabel, problemWords, stageWords, titleBadge, titleName, type StageTone } from "@/lib/requests";
import { titleUrl } from "@/lib/urls";

/*
 * What Discover, a title's page and Requests share: TMDB's titles as
 * cards, rows and grids, requests' stage pills, and why requests can't be
 * used.
 */

/** In a library, asked for, or declined: how a title stands for this account (nothing for none). */
function TitleBadge({ title }: { title: DiscoverTitle }) {
  const words = titleBadge(title);
  if (!words) return null;
  const icon =
    title.status === "library" || (title.status === "partial" && title.item_id !== null) ? (
      <Check className="size-3 text-success" strokeWidth={3} />
    ) : title.status === "declined" ? (
      <Ban className="size-3 text-danger" strokeWidth={2.5} />
    ) : (
      <Clock className="size-3 text-accent" strokeWidth={2.5} />
    );
  return (
    <span
      aria-hidden
      className="absolute left-1.5 top-1.5 inline-flex items-center gap-1 rounded bg-black/80 px-1.5 py-0.5 text-[10px] font-semibold leading-tight tracking-wide text-white shadow"
    >
      {icon}
      {words}
    </span>
  );
}

/** A TMDB title's poster, to its page, saying where it stands (in its name too, for screen readers). */
export const TitleCard = memo(function TitleCard({ title, width, fixed }: { title: DiscoverTitle; width: number; fixed?: boolean }) {
  const to = titleUrl(title.kind, title.tmdb_id);
  const subtitle = [title.year, kindLabel(title.kind)].filter(Boolean).join(" · ");
  return (
    <div className="group/card min-w-0" style={fixed ? { width } : undefined}>
      <Link
        to={to}
        aria-label={titleName(title)}
        className="relative block overflow-hidden rounded-md bg-surface-2 outline-none ring-1 ring-white/5 transition duration-200 hover:ring-2 hover:ring-accent focus-visible:ring-2 focus-visible:ring-accent"
      >
        <Artwork
          candidates={title.poster_url ? [title.poster_url] : []}
          width={width}
          alt=""
          title={title.title}
          className={cn(ASPECT_CLASS.poster, "w-full")}
          imgClassName="transition-transform duration-300 motion-safe:group-hover/card:scale-[1.04]"
        />
        <div className="pointer-events-none absolute inset-0 bg-black/0 transition-colors duration-200 group-hover/card:bg-black/30" />
        <TitleBadge title={title} />
      </Link>
      <div className="mt-2 px-0.5">
        {/* The poster above is the same link: Tab stops there once, not twice. */}
        <Link to={to} tabIndex={-1} aria-hidden className="block truncate text-sm font-medium text-fg transition-colors hover:text-accent" title={title.title}>
          {title.title}
        </Link>
        <div className="truncate text-xs text-muted" aria-hidden>
          {subtitle}
        </div>
      </div>
    </div>
  );
});

const GRID_GAP = 18;
const GRID_CARD = 150;

function useGridColumns() {
  const [ref, width] = useElementWidth<HTMLDivElement>();
  const cols = width > 0 ? Math.max(2, Math.floor((width + GRID_GAP) / (GRID_CARD + GRID_GAP))) : 6;
  const colWidth = width > 0 ? (width - GRID_GAP * (cols - 1)) / cols : GRID_CARD;
  return { ref, cols, colWidth };
}

/** Titles in whole columns as wide as the page; `more` loads the next page. */
export function TitleGrid({
  titles,
  more,
}: {
  titles: DiscoverTitle[];
  more?: { next: boolean; loading: boolean; load: () => void };
}) {
  const { ref, cols, colWidth } = useGridColumns();
  return (
    <div>
      <div ref={ref} className="grid gap-x-[18px] gap-y-6" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}>
        {titles.map((t) => (
          <TitleCard key={`${t.kind}-${t.tmdb_id}`} title={t} width={colWidth} />
        ))}
      </div>
      {more?.next ? (
        <div className="mt-8 flex justify-center">
          <Button variant="secondary" loading={more.loading} onClick={more.load}>
            Show more
          </Button>
        </div>
      ) : null}
    </div>
  );
}

export function TitleGridSkeleton({ count = 12 }: { count?: number }) {
  const { ref, cols } = useGridColumns();
  return (
    <div ref={ref} className="grid gap-x-[18px] gap-y-6" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}>
      {Array.from({ length: count }, (_, i) => (
        <CardSkeleton key={i} aspect="poster" />
      ))}
    </div>
  );
}

/**
 * Whether the element is on screen or about to be (a screen's height
 * ahead); once it was, it stays so. Without IntersectionObserver, it is.
 */
export function useInView<T extends Element>(): [RefObject<T | null>, boolean] {
  const ref = useRef<T>(null);
  const [seen, setSeen] = useState(() => typeof IntersectionObserver === "undefined");
  useEffect(() => {
    const el = ref.current;
    if (seen || !el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) setSeen(true);
      },
      { rootMargin: "100% 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [seen]);
  return [ref, seen];
}

/** How many titles a row loads before it is full: a page's worth. */
const ROW_TITLES = 20;

/**
 * One of TMDB's lists as a row that scrolls a page of cards at a time, with
 * a link to all of it. It loads once it comes into view, and loads on past
 * pages that came short until it is full.
 */
export function TitleRow({ list, title, link }: { list: DiscoverList; title: string; link: string }) {
  const [ref, inView] = useInView<HTMLDivElement>();
  const query = useDiscoverList(list, inView);
  useFillPages(query, ROW_TITLES);
  const titles = uniqueTitles(query.data?.pages);
  if (!inView || query.isPending) {
    return (
      <div ref={ref}>
        <HubRowSkeleton />
      </div>
    );
  }
  if (!query.data) {
    const problem = requestProblem(query.error);
    const words = problem ? problemWords(problem, { manage: false }) : null;
    return (
      <section className="px-4 sm:px-8">
        <h2 className="mb-2 text-lg font-semibold tracking-tight">{title}</h2>
        <div role="alert" className="flex flex-wrap items-center gap-3 text-sm">
          <span className="text-danger">{words ? words.title : `Couldn't load it: ${errorMessage(query.error)}`}</span>
          {!words || words.retry ? (
            <Button size="sm" loading={query.isFetching} onClick={() => void query.refetch()}>
              Retry
            </Button>
          ) : null}
        </div>
      </section>
    );
  }
  return titles.length ? <TitleCards titles={titles} title={title} link={link} /> : null;
}

/**
 * A row of poster-wide cards that scrolls a page of them at a time, with a
 * heading linked to all of them. `inset`: inside a page that already has
 * the side margin.
 */
export function ScrollRow({
  title,
  link,
  linkLabel = "See all",
  count,
  inset,
  children,
}: {
  title: string;
  link: string;
  linkLabel?: string;
  /** How many cards it holds (it measures again as that changes). */
  count: number;
  inset?: boolean;
  children: (width: number) => ReactNode;
}) {
  const { ref: scroller, width, perPage, gap } = useFittedRow([118, 160]);
  const [canLeft, setCanLeft] = useState(false);
  const [canRight, setCanRight] = useState(false);

  const update = useCallback(() => {
    const el = scroller.current;
    if (!el) return;
    setCanLeft(el.scrollLeft > 4);
    setCanRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 4);
  }, [scroller]);

  useLayoutEffect(() => {
    update();
  }, [update, count, width]);

  const scrollPage = (dir: 1 | -1) => scroller.current?.scrollBy({ left: dir * perPage * (width + gap), behavior: smoothScroll() });

  return (
    <section>
      <div className={cn("mb-2 flex items-center justify-between gap-4", !inset && "px-4 sm:px-8")}>
        <h2 className="truncate text-lg font-semibold tracking-tight">
          <Link to={link} className="transition-colors hover:text-accent">
            {title}
          </Link>
        </h2>
        <div className="flex shrink-0 items-center gap-1">
          <Link to={link} className="mr-2 text-xs font-medium text-muted transition-colors hover:text-accent">
            {linkLabel}
          </Link>
          {canLeft || canRight ? (
            <>
              <ArrowButton side="left" enabled={canLeft} onClick={() => scrollPage(-1)} />
              <ArrowButton side="right" enabled={canRight} onClick={() => scrollPage(1)} />
            </>
          ) : null}
        </div>
      </div>
      <div className={inset ? undefined : "px-4 sm:px-8"}>
        <div
          ref={scroller}
          onScroll={update}
          className="scrollbar-none -mx-1 flex snap-x snap-mandatory scroll-px-1 gap-3.5 overflow-x-auto px-1 pt-1 pb-2 sm:gap-4"
        >
          {children(width)}
        </div>
      </div>
    </section>
  );
}

/** A row of {@link ScrollRow}'s cards while they load, under its heading, as wide as they'll be. */
export function ScrollRowSkeleton({ title, inset }: { title: string; inset?: boolean }) {
  const { ref, width, perPage } = useFittedRow([118, 160]);
  return (
    <section aria-busy="true">
      <div className={cn("mb-2 flex h-8 items-center", !inset && "px-4 sm:px-8")}>
        <h2 className="truncate text-lg font-semibold tracking-tight">{title}</h2>
      </div>
      <div className={inset ? undefined : "px-4 sm:px-8"}>
        <div ref={ref} className="-mx-1 flex gap-3.5 overflow-hidden px-1 pt-1 pb-2 sm:gap-4">
          {Array.from({ length: Math.max(perPage, 4) }, (_, i) => (
            <div key={i} className="shrink-0">
              <CardSkeleton aspect="poster" width={width} fixed />
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

/** A row of TMDB's titles, as {@link ScrollRow} lays them out. */
export function TitleCards({ titles, ...row }: { titles: DiscoverTitle[]; title: string; link: string; linkLabel?: string; inset?: boolean }) {
  return (
    <ScrollRow {...row} count={titles.length}>
      {(width) =>
        titles.map((t) => (
          <div key={`${t.kind}-${t.tmdb_id}`} className="shrink-0 snap-start">
            <TitleCard title={t} width={width} fixed />
          </div>
        ))
      }
    </ScrollRow>
  );
}

const PILL_TONES: Record<StageTone, string> = {
  warning: "bg-warning/15 text-warning",
  info: "bg-info/15 text-info",
  success: "bg-success/15 text-success",
  danger: "bg-danger/15 text-danger",
  neutral: "bg-white/10 text-fg/80",
};

const BAR_TONES: Record<RequestDownloadState, string> = {
  queued: "bg-info/60",
  downloading: "bg-info",
  importing: "bg-info",
  paused: "bg-subtle",
  warning: "bg-danger",
};

type RequestDownloadState = NonNullable<MediaRequest["download"]>["state"];

/**
 * Where a request is on its way to a library, in its colour: a download's
 * progress as a bar with its value, and what more there is to say under it
 * (why a failed one failed, when it's tried again). `compact`: on one
 * line, cut short where it doesn't fit (under a poster).
 */
export function StagePill({
  request,
  manage,
  className,
  line = true,
  compact,
}: {
  request: MediaRequest;
  manage: boolean;
  className?: string;
  line?: boolean;
  compact?: boolean;
}) {
  const words = stageWords(request, manage);
  const download = words.download;
  return (
    <div className={cn("min-w-0 space-y-1.5", className)}>
      <span
        className={cn("inline-flex max-w-full items-center rounded-full px-2 py-0.5 text-xs font-semibold", PILL_TONES[words.tone])}
        title={compact ? words.label : undefined}
      >
        <span className={cn("min-w-0", compact ? "truncate" : "break-words")}>{words.label}</span>
      </span>
      {download ? (
        <div
          role="progressbar"
          aria-label={`${request.title}: ${words.label}`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(download.progress * 100)}
          className="h-1 w-full max-w-56 overflow-hidden rounded-full bg-white/10"
        >
          <div
            className={cn("h-full rounded-full transition-[width] duration-500", BAR_TONES[download.state])}
            style={{ width: `${Math.max(0, Math.min(1, download.progress)) * 100}%` }}
          />
        </div>
      ) : null}
      {line && words.line ? <p className="text-xs text-muted">{words.line}</p> : null}
    </div>
  );
}

/** "Details and pictures from TMDB.", at the foot of Discover and of every title's page. */
export function TmdbCredit({ className, full }: { className?: string; full?: boolean }) {
  return (
    <p className={cn("text-xs text-subtle", className)}>
      Details and pictures from TMDB.
      {full ? " This product uses the TMDB API but is not endorsed or certified by TMDB." : null}
    </p>
  );
}

const PROBLEM_ICONS: Record<RequestProblem["kind"], ReactNode> = {
  off: <Inbox />,
  tmdb_key: <KeyRound />,
  tmdb_down: <CloudOff />,
  no_access: <Ban />,
  sign_in: <LogIn />,
  restricted: <Ban />,
  declined: <Ban />,
  too_many: <Clock />,
  conflict: <CircleCheck />,
};

/**
 * Why a page of requests can't show: its problem's words (Retry only
 * where it may help; managers learn where to fix TMDB's key), or what
 * failed, with Retry, for anything else.
 */
export function ProblemState({
  error,
  problem = requestProblem(error),
  manage,
  title,
  onRetry,
  retrying,
  className = "min-h-[60vh]",
}: {
  error?: unknown;
  problem?: RequestProblem | null;
  manage: boolean;
  /** What couldn't be loaded, for errors that aren't about requests. */
  title?: string;
  onRetry?: () => void;
  retrying?: boolean;
  className?: string;
}) {
  if (!problem) {
    return (
      <EmptyState
        className={className}
        title={title ?? "Couldn't load it"}
        description={errorMessage(error)}
        action={
          onRetry ? (
            <Button loading={retrying} onClick={onRetry}>
              Retry
            </Button>
          ) : undefined
        }
      />
    );
  }
  const words = problemWords(problem, { manage });
  return (
    <EmptyState
      className={className}
      icon={PROBLEM_ICONS[problem.kind]}
      title={words.title}
      description={words.text ?? undefined}
      action={
        words.settings ? (
          <Link to={TMDB_KEY_SETTING} className={buttonClass("primary")}>
            Check the key
          </Link>
        ) : problem.kind === "sign_in" ? (
          <SignInAgain />
        ) : words.retry && onRetry ? (
          <Button loading={retrying} onClick={onRetry}>
            Retry
          </Button>
        ) : undefined
      }
    />
  );
}

/**
 * For a sign-in made before it could ask for titles (or approve them):
 * signs out, and back to this page once signed in again.
 */
export function SignInAgain({ size = "md" }: { size?: "sm" | "md" | "lg" }) {
  const logout = useLogout();
  const audio = useOptionalAudioPlayer();
  const navigate = useNavigate();
  const location = useLocation();
  return (
    <Button
      variant="primary"
      size={size}
      icon={<LogIn className={size === "lg" ? "size-5" : "size-4"} />}
      loading={logout.isPending}
      onClick={() => {
        const next = location.pathname + location.search;
        audio?.close();
        logout.mutate(undefined, { onSettled: () => void navigate(`/login?next=${encodeURIComponent(next)}`, { replace: true }) });
      }}
    >
      Sign in
    </Button>
  );
}

/**
 * Requests are off, or not for this account. Administrators of a server
 * where they're off may turn them on.
 */
export function RequestsUnavailable({ reason, admin }: { reason: "off" | "no_access"; admin: boolean }) {
  if (reason === "no_access") return <ProblemState problem={{ kind: "no_access" }} manage={false} />;
  if (!admin) return <ProblemState problem={{ kind: "off" }} manage={false} />;
  return (
    <EmptyState
      className="min-h-[60vh]"
      icon={<Inbox />}
      title="Requests are off"
      description="Turn them on to let people ask for movies and shows, and send what you approve to Radarr and Sonarr."
      action={
        <Link to="/settings/requests" className={buttonClass("primary")}>
          Set up requests
        </Link>
      }
    />
  );
}
