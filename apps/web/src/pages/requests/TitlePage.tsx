import { useCallback, useLayoutEffect, useState } from "react";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { Link, useParams, useSearchParams } from "react-router";
import { Ban, Bell, Check, Clock, Play, Plus, UserMinus } from "lucide-react";
import { isApiError } from "@/api/client";
import {
  useAskRequest,
  useDiscoverTitle,
  useRequestSummary,
  useUnwatchRequest,
  type DiscoverDetail,
  type DiscoverSeason,
  type MediaRequest,
  type RequestSummary,
  type TitleKind,
} from "@/api/requests";
import { useAuth } from "@/auth";
import { Artwork } from "@/components/Artwork";
import { ArrowButton, useFittedRow } from "@/components/HubRow";
import { PersonPhoto } from "@/components/PersonCard";
import { useToast } from "@/components/Toast";
import { Badge, Button, buttonClass, SectionTitle, Skeleton } from "@/components/ui";
import { cn } from "@/lib/cn";
import { formatDate, formatDuration } from "@/lib/format";
import { smoothScroll, useDocumentTitle } from "@/lib/hooks";
import { allowanceText, askedText, kindLabel, mainAction, seasonStateText, spent } from "@/lib/requests";
import { itemUrl } from "@/lib/urls";
import { NotFoundPage } from "../NotFoundPage";
import { ContentRating, Genres, MetaLine, RatingBadge, Summary } from "../item/shared";
import { failure, RequestCard } from "./RequestCard";
import { SeasonPicker } from "./SeasonPicker";
import { ProblemState, SignInAgain, StagePill, TmdbCredit } from "./shared";

function TitleSkeleton() {
  return (
    <div className="px-4 pb-16 pt-6 sm:px-8 sm:pt-[10vh] lg:pt-[14vh]" aria-busy>
      <div className="flex flex-col gap-8 sm:flex-row sm:gap-10">
        <Skeleton className="hidden aspect-[2/3] w-60 shrink-0 rounded-lg sm:block" />
        <div className="flex-1 space-y-4">
          <Skeleton className="h-10 w-2/3" />
          <Skeleton className="h-4 w-1/3" />
          <Skeleton className="h-12 w-48" />
          <Skeleton className="h-4 w-full max-w-2xl" />
          <Skeleton className="h-4 w-5/6 max-w-2xl" />
        </div>
      </div>
    </div>
  );
}

function Backdrop({ title }: { title: DiscoverDetail }) {
  return (
    <div className="pointer-events-none absolute inset-x-0 top-0 h-[78vh] max-h-[760px] overflow-hidden" aria-hidden>
      <Artwork
        candidates={title.backdrop_url ? [title.backdrop_url] : []}
        width={1280}
        alt=""
        title={title.title}
        bare
        eager
        className="size-full opacity-70"
        imgClassName="object-cover object-[center_20%]"
      />
      <div className="absolute inset-0 bg-linear-to-t from-bg via-bg/75 to-bg/10" />
      <div className="absolute inset-0 bg-linear-to-r from-bg/95 via-bg/50 to-transparent" />
    </div>
  );
}

/** "Coming Nov 4, 2026" for a release still to come, else its year. */
function released(title: DiscoverDetail, t: TFunction<"requests">): string | null {
  const date = title.release_date;
  if (date && new Date(`${date}T00:00:00`).getTime() > Date.now()) return t("title.coming", { date: formatDate(date) });
  return title.year ? String(title.year) : null;
}

/**
 * The one thing a title's page offers (the first that applies), with
 * its request's stage where there is one, and what's left to ask for
 * once nothing is. `onPick` opens the season picker. `listed`: the page
 * lists the title's requests, each with its stage and what may be done
 * to it, so this says neither again.
 */
export function MainActions({
  title,
  summary,
  onPick,
  size = "lg",
  listed,
}: {
  title: DiscoverDetail;
  summary: RequestSummary | undefined;
  onPick: () => void;
  size?: "md" | "lg";
  listed?: boolean;
}) {
  const { t } = useTranslation("requests");
  const toast = useToast();
  const ask = useAskRequest();
  const unwatch = useUnwatchRequest();
  const action = mainAction(title, summary ?? { sign_in_again: null });
  const manage = summary?.manage ?? false;
  const off = summary?.access === "off";
  const used = spent(title, title.kind);
  const allowance = used ? allowanceText({ ...title, movies_limit: summary?.movies_limit, seasons_limit: summary?.seasons_limit }, title.kind) : null;
  const icon = size === "lg" ? "size-5" : "size-4";
  const fail = (e: unknown) => toast.error(failure(e, summary ?? { manage: false }, title.kind));

  const request = () => {
    if (title.kind === "show") return onPick();
    ask.mutate({ kind: title.kind, tmdb_id: title.tmdb_id }, { onSuccess: (asked) => toast.success(askedText(asked)), onError: fail });
  };
  const tellMeToo = (seasons: number[] | null) =>
    ask.mutate(
      { kind: title.kind, tmdb_id: title.tmdb_id, ...(seasons?.length ? { seasons } : {}) },
      { onSuccess: (asked) => toast.success(askedText(asked)), onError: fail },
    );

  const open = (itemId: number | null, primary = true) =>
    itemId !== null ? (
      <Link to={itemUrl(itemId)} className={buttonClass(primary ? "primary" : "secondary", size)}>
        <Play className={cn(icon, "fill-current")} />
        {t("title.open")}
      </Link>
    ) : null;
  const moreSeasons = (
    <Button variant="secondary" size={size} icon={<Plus className={icon} />} disabled={used} onClick={onPick}>
      {t("title.requestMoreSeasons")}
    </Button>
  );
  const notOpen = <p className="text-sm text-muted">{t("title.notOpen")}</p>;
  const stage = (r: MediaRequest | null) => (r && !listed ? <StagePill request={r} manage={manage} className="basis-full" /> : null);

  let body;
  switch (action.kind) {
    case "sign_in":
      body = (
        <div className="space-y-2">
          <p className="font-medium">{t("title.signInToAsk")}</p>
          <SignInAgain size={size} />
        </div>
      );
      break;
    case "restricted":
      body = (
        <p className="flex items-center gap-2 text-sm text-muted">
          <Ban className="size-4 shrink-0 text-danger" />
          {action.text}
        </p>
      );
      break;
    case "open":
      body = open(action.itemId) ?? <Badge tone="success">{t("title.inLibrary")}</Badge>;
      break;
    case "declined":
      body = (
        <>
          <p className="flex basis-full items-start gap-2 text-sm text-muted">
            <Ban className="mt-0.5 size-4 shrink-0 text-danger" />
            {action.text}
          </p>
          {action.more ? (off ? notOpen : moreSeasons) : null}
        </>
      );
      break;
    case "requested":
      body = (
        <>
          <span className={buttonClass("secondary", size, "pointer-events-none text-muted")} aria-disabled>
            <Check className={icon} />
            {t("title.requested")}
          </span>
          {stage(action.request)}
        </>
      );
      break;
    case "following":
      body = (
        <>
          <span className={buttonClass("secondary", size, "pointer-events-none text-muted")} aria-disabled>
            <Bell className={icon} />
            {t("title.following")}
          </span>
          {action.request && !listed ? (
            <Button
              variant="ghost"
              size={size}
              icon={<UserMinus className={icon} />}
              loading={unwatch.isPending}
              onClick={() => {
                const followed = action.request;
                if (followed) unwatch.mutate(followed, { onError: fail });
              }}
            >
              {t("title.stopFollowing")}
            </Button>
          ) : null}
          {stage(action.request)}
        </>
      );
      break;
    case "tell_me_too":
      body = off ? (
        notOpen
      ) : (
        <>
          <Button variant="primary" size={size} icon={<Bell className={icon} />} loading={ask.isPending} onClick={() => tellMeToo(action.seasons)}>
            {t("title.tellMeToo")}
          </Button>
          <p className="text-sm text-muted">{t("title.askedAlready")}</p>
        </>
      );
      break;
    case "partial":
      body = (
        <>
          {open(action.itemId)}
          {action.more ? (off ? notOpen : moreSeasons) : null}
        </>
      );
      break;
    case "request":
      body = off ? (
        notOpen
      ) : (
        <Button variant="primary" size={size} icon={<Plus className={icon} />} loading={ask.isPending} disabled={used} onClick={request}>
          {t("shared.request")}
        </Button>
      );
      break;
  }

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2.5">{body}</div>
      {allowance && !off && action.kind !== "open" && action.kind !== "restricted" ? (
        <p className="flex items-start gap-1.5 text-xs text-muted">
          <Clock className="mt-px size-3.5 shrink-0 text-warning" aria-hidden />
          {allowance.text}
        </p>
      ) : null}
    </div>
  );
}

/** A season's state, from the server's word for it: its request's stage where this account sees the request. */
function SeasonState({ season, title, manage }: { season: DiscoverSeason; title: DiscoverDetail; manage: boolean }) {
  const request = season.request_id !== null ? title.requests.find((r) => r.id === season.request_id) : undefined;
  if (season.state === "requested" && request) {
    // A request partly ready speaks of the seasons that are: this one isn't, so it says how its download goes, briefly.
    if (request.stage !== "partial") return <StagePill request={request} manage={manage} line={false} className="shrink-0" />;
    if (request.download) {
      const download = { ...request.download, eta: null };
      return <StagePill request={{ ...request, stage: "downloading", download }} manage={manage} line={false} className="shrink-0" />;
    }
  }
  const text = seasonStateText(season);
  if (!text) return null;
  if (season.state === "library") return <Badge tone="success">{text}</Badge>;
  // Declined, on seasonStateText's terms: its words are in the language shown, so they aren't compared.
  if (season.declined && season.state !== "requested") return <Badge tone="danger">{text}</Badge>;
  if (season.state === "requested") {
    return (
      <Badge tone="info">
        <Clock className="size-3" /> {text}
      </Badge>
    );
  }
  if (season.state === "partial") return <Badge>{text}</Badge>;
  return <span className="shrink-0 text-xs text-subtle">{text}</span>;
}

function SeasonRow({ season, title, manage }: { season: DiscoverSeason; title: DiscoverDetail; manage: boolean }) {
  const { t } = useTranslation(["requests", "common"]);
  return (
    <li className="flex items-center gap-3 rounded-lg border border-border bg-surface/80 px-3 py-2.5">
      <Artwork
        candidates={season.poster_url ? [season.poster_url] : []}
        width={40}
        alt=""
        title=""
        bare
        className="aspect-[2/3] w-10 shrink-0 rounded ring-1 ring-white/5"
      />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium">
          {season.name || (season.number === 0 ? t("common:format.specials") : t("common:format.seasonNumber", { number: season.number }))}
        </div>
        <div className="truncate text-xs text-subtle">
          {[t("counts.episodes", { count: season.episodes }), season.air_date ? season.air_date.slice(0, 4) : null].filter(Boolean).join(" · ")}
        </div>
      </div>
      <SeasonState season={season} title={title} manage={manage} />
    </li>
  );
}

/** TMDB's cast: no pages of their own here (people have them only in a library). */
function CastRow({ cast }: { cast: DiscoverDetail["cast"] }) {
  const { t } = useTranslation("requests");
  const { ref, width, perPage, gap } = useFittedRow([96, 120]);
  const [canLeft, setCanLeft] = useState(false);
  const [canRight, setCanRight] = useState(false);

  const update = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    setCanLeft(el.scrollLeft > 4);
    setCanRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 4);
  }, [ref]);

  useLayoutEffect(() => {
    update();
  }, [update, cast.length, width]);

  const scrollPage = (dir: 1 | -1) => ref.current?.scrollBy({ left: dir * perPage * (width + gap), behavior: smoothScroll() });

  return (
    <section className="mt-12">
      <SectionTitle
        action={
          canLeft || canRight ? (
            <div className="flex shrink-0 items-center gap-1">
              <ArrowButton side="left" enabled={canLeft} onClick={() => scrollPage(-1)} />
              <ArrowButton side="right" enabled={canRight} onClick={() => scrollPage(1)} />
            </div>
          ) : null
        }
      >
        {t("title.cast")}
      </SectionTitle>
      <div
        ref={ref}
        onScroll={update}
        className="scrollbar-none -mx-1 flex snap-x snap-mandatory scroll-px-1 gap-3.5 overflow-x-auto px-1 pt-1 pb-2 sm:gap-4"
      >
        {cast.map((c, i) => (
          <div key={`${c.name}-${i}`} className="shrink-0 snap-start text-center" style={{ width }}>
            <PersonPhoto name={c.name} url={c.photo_url} size={width} className="w-full" />
            <div className="mt-2 truncate px-0.5 text-sm font-medium" title={c.name}>
              {c.name}
            </div>
            {c.character ? (
              <div className="truncate px-0.5 text-xs text-muted" title={c.character}>
                {c.character}
              </div>
            ) : (
              <div className="h-4" />
            )}
          </div>
        ))}
      </div>
    </section>
  );
}

function TitleView({ title, picking, setPicking }: { title: DiscoverDetail; picking: boolean; setPicking: (open: boolean) => void }) {
  const { t } = useTranslation("requests");
  const summary = useRequestSummary();
  const show = title.kind === "show";
  const numbered = (title.seasons ?? []).filter((s) => s.number > 0);
  const manage = summary.data?.manage ?? false;
  const listed = title.requests.length > 0 && !!summary.data;

  return (
    <div className="relative min-h-[calc(100vh-4rem)] animate-fade-in">
      <Backdrop title={title} />
      <div className="relative px-4 pb-16 pt-6 sm:px-8 sm:pt-[10vh] lg:pt-[14vh]">
        <div className="flex flex-col gap-6 sm:flex-row sm:gap-10">
          <div className="hidden w-52 shrink-0 sm:block md:w-60 lg:w-64">
            <Artwork
              candidates={title.poster_url ? [title.poster_url] : []}
              width={256}
              alt={title.title}
              title={title.title}
              eager
              className="aspect-[2/3] w-full rounded-lg shadow-2xl shadow-black/60 ring-1 ring-white/10"
            />
          </div>
          <div className="min-w-0 flex-1 space-y-4">
            <div>
              <h1 className="text-3xl font-bold leading-tight tracking-tight text-balance sm:text-4xl">{title.title}</h1>
              {title.original_title ? <p className="mt-1 text-sm text-muted">{title.original_title}</p> : null}
            </div>
            <MetaLine
              parts={[
                kindLabel(title.kind),
                released(title, t),
                title.runtime_minutes ? formatDuration(title.runtime_minutes * 60_000) : null,
                show && numbered.length ? t("counts.seasons", { count: numbered.length }) : null,
                title.certification ? <ContentRating key="cr" value={title.certification} /> : null,
                title.rating != null && title.rating > 0 ? <RatingBadge key="r" rating={title.rating} /> : null,
                title.network,
                title.show_status,
              ]}
            />
            <Genres genres={title.genres} />
            <div className="pt-1">
              <MainActions title={title} summary={summary.data} listed={listed} onPick={() => setPicking(true)} />
            </div>
            {title.tagline ? <p className="text-base italic text-fg/75">{title.tagline}</p> : null}
            <Summary text={title.overview} />
          </div>
        </div>

        {listed && summary.data ? (
          <section className="mt-12">
            <SectionTitle>
              {t("title.requests", { count: title.requests.length })}{" "}
              {title.requests.length > 1 ? <span className="ml-1 text-sm font-normal text-subtle">{title.requests.length}</span> : null}
            </SectionTitle>
            <ul className="divide-y divide-white/[0.05] rounded-lg border border-border bg-surface/80">
              {title.requests.map((r) => (
                <RequestCard key={r.id} request={r} summary={summary.data} showTitle={false} />
              ))}
            </ul>
          </section>
        ) : null}

        {show && title.seasons?.length ? (
          <section className="mt-12">
            <SectionTitle>{t("shared.seasons")}</SectionTitle>
            <ul className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
              {[...numbered, ...title.seasons.filter((s) => s.number === 0)].map((s) => (
                <SeasonRow key={s.number} season={s} title={title} manage={manage} />
              ))}
            </ul>
          </section>
        ) : null}

        {title.cast.length ? <CastRow cast={title.cast} /> : null}

        <TmdbCredit className="mt-12" />
      </div>
      {show ? <SeasonPicker title={title} summary={summary.data} open={picking} onClose={() => setPicking(false)} /> : null}
    </div>
  );
}

/**
 * A movie or show on TMDB (`/discover/movie/603`): what it is, how it
 * stands for this account, and the one thing to do about it. `?pick=1`
 * opens a show's season picker (Request more seasons, from a library).
 */
export function TitlePage() {
  const { t } = useTranslation("requests");
  const me = useAuth();
  const params = useParams();
  const [search, setSearch] = useSearchParams();
  const kind: TitleKind | null = params.kind === "movie" || params.kind === "show" ? params.kind : null;
  const tmdbId = Number(params.tmdbId);
  const valid = kind !== null && Number.isInteger(tmdbId) && tmdbId > 0;
  const summary = useRequestSummary();
  const title = useDiscoverTitle(kind ?? "movie", valid ? tmdbId : 0, summary.data?.sign_in_again !== "requests");
  useDocumentTitle(title.data?.title);
  const picking = search.get("pick") === "1";
  const setPicking = (open: boolean) => {
    const next = new URLSearchParams(search);
    if (open) next.set("pick", "1");
    else next.delete("pick");
    setSearch(next, { replace: true });
  };

  if (!valid) return <NotFoundPage />;
  // This sign-in can't ask: it says so, and nothing more.
  if (summary.data?.sign_in_again === "requests") return <ProblemState problem={{ kind: "sign_in" }} manage={false} />;
  if (title.isPending) return <TitleSkeleton />;
  // Failed before anything came (a refresh that fails keeps what's shown).
  if (!title.data) {
    if (isApiError(title.error, 404) && !isApiError(summary.error, 404)) return <NotFoundPage />;
    return (
      <ProblemState
        error={title.error}
        manage={summary.data?.manage ?? me.is_admin}
        title={t("title.loadFailed")}
        retrying={title.isFetching}
        onRetry={() => void title.refetch()}
      />
    );
  }
  // Keyed, so what was being picked doesn't carry over to another title.
  return <TitleView key={`${title.data.kind}-${title.data.tmdb_id}`} title={title.data} picking={picking} setPicking={setPicking} />;
}
