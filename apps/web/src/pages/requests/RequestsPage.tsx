import { useEffect, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router";
import { Ban, Check, CircleCheck, Compass, Inbox, Search, SearchX, X } from "lucide-react";
import { isApiError } from "@/api/client";
import {
  hasRequests,
  mayRequest,
  requestFilter,
  SEARCH_MAX,
  useDecideMany,
  useMarkSeen,
  useRequests,
  useRequestSummary,
  type MediaRequest,
  type RequestFilter,
  type RequestStatus,
  type RequestSummary,
  type TitleKind,
} from "@/api/requests";
import { useAuth } from "@/auth";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button, buttonClass, EmptyState, Field, Select, Skeleton, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";
import { pluralize } from "@/lib/format";
import { useDebouncedValue, useDocumentTitle, useOnChange } from "@/lib/hooks";
import { allowanceText } from "@/lib/requests";
import { discoverUrl } from "@/lib/urls";
import { failure, RequestCard } from "./RequestCard";
import { ProblemState, RequestsUnavailable, SignInAgain } from "./shared";

const STATUSES: { status: RequestStatus; label: string }[] = [
  { status: "pending", label: "Pending" },
  { status: "approved", label: "Approved" },
  { status: "available", label: "Available" },
  { status: "declined", label: "Declined" },
  { status: "failed", label: "Failed" },
];

/** Managers' views: what waits for them (the default), everyone's, or their own. */
type View = "attention" | "all" | "mine";

function viewOf(value: string | null, manage: boolean): View {
  if (!manage) return "mine";
  return value === "all" || value === "mine" ? value : "attention";
}

function statusOf(value: string | null): RequestStatus | null {
  return STATUSES.find((s) => s.status === value)?.status ?? null;
}

/** A filter that is on or off, as a pressed button. */
function FilterButton({ pressed, onClick, children, count }: { pressed: boolean; onClick: () => void; children: string; count?: number | null }) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={cn(
        "inline-flex h-8 items-center gap-1.5 rounded-full border px-3 text-sm font-medium transition-colors",
        pressed ? "border-accent/60 bg-accent/15 text-fg" : "border-border text-muted hover:border-border-strong hover:text-fg",
      )}
    >
      {children}
      {/* The space outside the span, as the name is made of the text: "Pending 3". */}
      {count ? (
        <>
          {" "}
          <span className="text-xs tabular-nums text-subtle">{count}</span>
        </>
      ) : null}
    </button>
  );
}

/** Titles containing what's typed, kept in the address (`?q=`). */
function TitleSearch({ q, onChange }: { q: string; onChange: (q: string) => void }) {
  const [value, setValue] = useState(q);
  const debounced = useDebouncedValue(value, 300);
  const inputRef = useRef<HTMLInputElement>(null);
  useOnChange(q, (next) => {
    if (next !== debounced.trim()) setValue(next);
  });
  useEffect(() => {
    if (debounced.trim() !== q) onChange(debounced.trim());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [debounced]);
  return (
    <div className="relative min-w-0 flex-1 sm:w-56 sm:flex-none">
      <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-subtle" />
      <input
        ref={inputRef}
        type="search"
        value={value}
        maxLength={SEARCH_MAX}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") setValue("");
        }}
        placeholder="Find a title"
        aria-label="Requests for titles containing"
        className="h-9 w-full rounded-full border border-border bg-surface-2 pl-9 pr-8 text-sm text-fg placeholder:text-subtle transition-colors focus:border-accent/70 focus:bg-surface-3 focus:outline-none [&::-webkit-search-cancel-button]:hidden"
      />
      {value ? (
        <button
          type="button"
          aria-label="Clear"
          onClick={() => {
            setValue("");
            inputRef.current?.focus();
          }}
          className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-full p-1 text-subtle hover:text-fg"
        >
          <X className="size-3.5" />
        </button>
      ) : null}
    </div>
  );
}

/** What a list with nothing in it says. */
function emptyText(view: View, filter: RequestFilter, manage: boolean): string {
  if (filter.q || filter.kind || filter.userId) return "No requests match.";
  if (view === "attention") return "Nothing waits for you.";
  const whose = manage && view === "all" ? "" : " of yours";
  switch (filter.status[0]) {
    case "pending":
      return `No requests${whose} wait for a decision.`;
    case "approved":
      return `No requests${whose} are on their way.`;
    case "available":
      return `No requests${whose} are ready to watch yet.`;
    case "declined":
      return `No requests${whose} were declined.`;
    case "failed":
      return `No requests${whose} failed.`;
    default:
      return manage && view === "all" ? "No one has asked for anything yet." : "You haven't asked for anything yet.";
  }
}

/** Declining several at once, with one reason for all, if any. */
function DeclineManyDialog({ count, open, onClose, onDecline, busy }: { count: number; open: boolean; onClose: () => void; onDecline: (reason?: string) => void; busy: boolean }) {
  const [reason, setReason] = useState("");
  useOnChange(open, (o) => {
    if (o) setReason("");
  });
  return (
    <Modal
      open={open}
      onClose={onClose}
      locked={busy}
      size="sm"
      title={`Decline ${pluralize(count, "request")}?`}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button variant="danger" loading={busy} onClick={() => onDecline(reason.trim() || undefined)}>
            Decline
          </Button>
        </>
      }
    >
      <Field label="Reason" htmlFor="decline-many" hint="Optional. Those who asked see it.">
        <TextInput id="decline-many" maxLength={500} autoComplete="off" value={reason} onChange={(e) => setReason(e.target.value)} data-autofocus />
      </Field>
    </Modal>
  );
}

/** Approving or declining the requests picked, one after another, saying how it went. */
function BulkBar({ picked, clear, summary }: { picked: MediaRequest[]; clear: () => void; summary: RequestSummary }) {
  const decide = useDecideMany();
  const toast = useToast();
  const [declining, setDeclining] = useState(false);
  const approvable = picked.filter((r) => r.actions.includes("approve") || r.actions.includes("retry"));
  const declinable = picked.filter((r) => r.actions.includes("decline"));

  const run = (decision: "approve" | "decline", requests: MediaRequest[], reason?: string) =>
    decide.mutate(
      { requests, decision, reason },
      {
        onSuccess: (done) => {
          const failed = done.filter((d) => d.error);
          const verb = decision === "approve" ? "Approved" : "Declined";
          if (!failed.length) toast.success(`${verb} ${pluralize(done.length, "request")}.`);
          else {
            const [first] = failed;
            const title = requests.find((r) => r.id === first?.request.id)?.title ?? "";
            toast.error(`${verb} ${done.length - failed.length} of ${done.length}. “${title}”: ${failure(first?.error, summary, first?.request.kind ?? "movie")}`);
          }
          setDeclining(false);
          clear();
        },
      },
    );

  return (
    <div
      role="region"
      aria-label="Requests picked"
      className="sticky bottom-4 z-20 flex flex-wrap items-center gap-x-3 gap-y-2.5 rounded-lg border border-border-strong bg-surface-2 px-4 py-3 shadow-xl shadow-black/50"
    >
      <span className="min-w-0 flex-1 text-sm font-medium">{pluralize(picked.length, "request")} picked</span>
      <Button variant="ghost" size="sm" onClick={clear} disabled={decide.isPending}>
        Clear
      </Button>
      {/* On a phone, the decisions take a line of their own, half each. */}
      <div className="flex w-full gap-2 sm:w-auto">
        <Button
          variant="outline"
          size="sm"
          className="flex-1 sm:flex-none"
          icon={<Ban className="size-3.5" />}
          disabled={!declinable.length || decide.isPending}
          onClick={() => setDeclining(true)}
        >
          Decline {declinable.length !== picked.length ? declinable.length : ""}
        </Button>
        <Button
          variant="primary"
          size="sm"
          className="flex-1 sm:flex-none"
          icon={<Check className="size-4" />}
          disabled={!approvable.length}
          loading={decide.isPending && !declining}
          onClick={() => run("approve", approvable)}
        >
          Approve {approvable.length !== picked.length ? approvable.length : ""}
        </Button>
      </div>
      <DeclineManyDialog
        count={declinable.length}
        open={declining}
        busy={decide.isPending}
        onClose={() => setDeclining(false)}
        onDecline={(reason) => run("decline", declinable, reason)}
      />
    </div>
  );
}

/** The list as it will be: a poster, a title, a stage and who asked, in each row. */
function ListSkeleton() {
  return (
    <div className="divide-y divide-white/[0.05] rounded-lg border border-border bg-surface" aria-busy>
      {Array.from({ length: 4 }, (_, i) => (
        <div key={i} className="flex gap-3 px-4 py-3.5">
          <Skeleton className="aspect-[2/3] w-14 shrink-0 rounded" />
          <div className="min-w-0 flex-1 space-y-2.5 pt-1">
            <Skeleton className="h-4 w-48 max-w-full" />
            <Skeleton className="h-5 w-32 rounded-full" />
            <Skeleton className="h-3 w-40 max-w-full" />
          </div>
        </div>
      ))}
    </div>
  );
}

function RequestsView({ summary }: { summary: RequestSummary }) {
  const manage = summary.manage;
  const [params, setParams] = useSearchParams();
  const view = viewOf(params.get("view"), manage);
  const status = statusOf(params.get("status"));
  const kind: TitleKind | null = params.get("kind") === "movie" || params.get("kind") === "show" ? (params.get("kind") as TitleKind) : null;
  const userId = manage && Number(params.get("user")) > 0 ? Number(params.get("user")) : null;
  const q = params.get("q") ?? "";
  const filter = requestFilter(
    view === "attention"
      ? { attention: true, order: "oldest", kind, userId, q }
      : { status: status ? [status] : [], kind, userId: view === "all" ? userId : null, q, mine: view === "mine" && manage },
  );
  const requests = useRequests(filter);
  const shown = requests.data?.pages.flatMap((p) => p.requests) ?? [];
  const first = requests.data?.pages[0];
  const people = first?.people ?? [];
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const { mutate: markSeen } = useMarkSeen();
  const allowance = !manage ? allowanceText(summary) : null;

  // News is seen once the requests that have it show: marked for each piece of news as it shows (a request's
  // `updated_at` moves with its news), so news that comes while the page is open is marked too, and news a
  // filter hides isn't.
  const news = shown
    .filter((r) => r.unseen)
    .map((r) => `${r.id}:${r.updated_at}`)
    .join();
  const marked = useRef("");
  useEffect(() => {
    if (!news || news === marked.current) return;
    marked.current = news;
    markSeen();
  }, [news, markSeen]);

  // What was picked goes as the list changes to another.
  useOnChange(JSON.stringify(filter), () => setPicked(new Set()));

  const setParam = (key: string, value: string | null) => {
    const p = new URLSearchParams(params);
    if (value === null || value === "") p.delete(key);
    else p.set(key, value);
    setParams(p, { replace: true });
  };
  const decidable = (r: MediaRequest) => r.actions.includes("approve") || r.actions.includes("retry") || r.actions.includes("decline");
  const pickedRequests = shown.filter((r) => picked.has(r.id));

  if (requests.isError && !requests.data) {
    return (
      <ProblemState
        className="py-16"
        error={requests.error}
        manage={manage}
        title="Couldn't load the requests"
        retrying={requests.isFetching}
        onRetry={() => void requests.refetch()}
      />
    );
  }

  const setView = (v: View) => {
    const p = new URLSearchParams(params);
    if (v === "attention") p.delete("view");
    else p.set("view", v);
    p.delete("status");
    setParams(p, { replace: true });
  };
  const filtered = filter.status.length > 0 || filter.kind !== null || filter.userId !== null || filter.q !== "";
  const clearFilters = () => {
    const p = new URLSearchParams(params);
    for (const key of ["status", "kind", "user", "q"]) p.delete(key);
    setParams(p, { replace: true });
  };
  // Someone who never asked for anything has nothing to filter.
  const filters = manage || summary.own > 0 || filtered;
  const pickable = manage && shown.some(decidable);

  const attention = summary.pending + summary.failed;
  // Managers' by the tabs; others', after the filters (first, on a phone).
  const search = (
    <div className={cn("flex w-full items-center gap-2 sm:ml-auto sm:w-auto", !manage && "max-sm:order-first")}>
      {manage && view !== "mine" && (people.length > 1 || userId !== null) ? (
        <Select aria-label="Who asked" className="max-w-40 shrink-0" value={userId ?? ""} onChange={(e) => setParam("user", e.target.value || null)}>
          <option value="">Everyone</option>
          {people.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </Select>
      ) : null}
      <TitleSearch q={q} onChange={(next) => setParam("q", next || null)} />
    </div>
  );
  return (
    <div className="space-y-5">
      {manage ? (
        <div className="flex flex-wrap items-center gap-3">
          <div className="scrollbar-none max-w-full overflow-x-auto">
            <div className="flex w-max rounded-lg bg-surface-2 p-1" role="tablist" aria-label="Requests">
              {(
                [
                  ["attention", "Needs attention", attention],
                  ["all", "All requests", null],
                  ["mine", "Yours", summary.unseen || null],
                ] as const
              ).map(([v, label, count]) => (
                <button
                  key={v}
                  type="button"
                  role="tab"
                  aria-selected={view === v}
                  onClick={() => setView(v)}
                  className={cn(
                    "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
                    view === v ? "bg-surface-3 text-fg shadow" : "text-muted hover:text-fg",
                  )}
                >
                  {label}
                  {count ? (
                    <>
                      {" "}
                      <span
                        className={cn(
                          "min-w-5 rounded-full px-1.5 text-center text-[11px] font-semibold tabular-nums",
                          v === "attention" && summary.failed > 0 ? "bg-danger text-white" : v === "attention" ? "bg-warning text-black" : "bg-white/10 text-fg/80",
                        )}
                      >
                        {count}
                      </span>
                    </>
                  ) : null}
                </button>
              ))}
            </div>
          </div>
          {search}
        </div>
      ) : null}

      {filters ? (
        <div className="flex flex-wrap items-center gap-2">
          {view !== "attention" ? (
            <div className="flex flex-wrap gap-2" role="group" aria-label="Status">
              <FilterButton pressed={status === null} onClick={() => setParam("status", null)}>
                All
              </FilterButton>
              {STATUSES.map((s) => (
                <FilterButton key={s.status} pressed={status === s.status} count={first?.counts[s.status]} onClick={() => setParam("status", status === s.status ? null : s.status)}>
                  {s.label}
                </FilterButton>
              ))}
            </div>
          ) : null}
          <div className="flex flex-wrap gap-2" role="group" aria-label="Kind">
            <FilterButton pressed={kind === "movie"} onClick={() => setParam("kind", kind === "movie" ? null : "movie")}>
              Movies
            </FilterButton>
            <FilterButton pressed={kind === "show"} onClick={() => setParam("kind", kind === "show" ? null : "show")}>
              Shows
            </FilterButton>
          </div>
          {manage ? null : search}
        </div>
      ) : null}

      {allowance && summary.access !== "off" ? (
        <p className="text-xs text-subtle">
          {allowance.text}
          {allowance.limits ? ` ${allowance.limits}` : ""}
        </p>
      ) : null}

      {requests.isPending ? (
        <ListSkeleton />
      ) : shown.length === 0 ? (
        filtered ? (
          <EmptyState
            icon={<SearchX />}
            title={emptyText(view, filter, manage)}
            action={
              <Button variant="outline" onClick={clearFilters}>
                Clear filters
              </Button>
            }
          />
        ) : view === "attention" ? (
          <EmptyState
            icon={<CircleCheck />}
            title={emptyText(view, filter, manage)}
            description="New requests, and those that couldn't be sent, show up here."
            action={
              <Button variant="outline" onClick={() => setView("all")}>
                See all requests
              </Button>
            }
          />
        ) : (
          <EmptyState
            icon={<Inbox />}
            title={emptyText(view, filter, manage)}
            action={
              mayRequest(summary) && view === "mine" ? (
                <Link to={discoverUrl()} className={buttonClass("primary")}>
                  Find something to ask for
                </Link>
              ) : undefined
            }
          />
        )
      ) : (
        <div className="space-y-3">
          <ul className="divide-y divide-white/[0.05] rounded-lg border border-border bg-surface" aria-busy={requests.isFetching}>
            {shown.map((r) => (
              <RequestCard
                key={r.id}
                request={r}
                summary={summary}
                checkboxes={pickable}
                selected={picked.has(r.id)}
                onSelect={
                  manage && decidable(r)
                    ? (on) =>
                        setPicked((p) => {
                          const next = new Set(p);
                          if (on) next.add(r.id);
                          else next.delete(r.id);
                          return next;
                        })
                    : undefined
                }
              />
            ))}
          </ul>
          {requests.hasNextPage ? (
            <Button variant="ghost" size="sm" loading={requests.isFetchingNextPage} onClick={() => void requests.fetchNextPage()}>
              Show more
            </Button>
          ) : null}
          {pickedRequests.length ? <BulkBar picked={pickedRequests} clear={() => setPicked(new Set())} summary={summary} /> : null}
        </div>
      )}
    </div>
  );
}

/**
 * Requests (`/requests`): for managers, what needs them first (pending
 * requests, and failed ones the server won't try again), then everyone's
 * and their own; for others, what they asked for or follow. Filters by
 * status, kind, person and title, with what may be done to each.
 */
export function RequestsPage() {
  const me = useAuth();
  const summary = useRequestSummary();
  const data = summary.data;
  useDocumentTitle(data?.manage ? "Requests" : "Your requests");

  let body;
  if (summary.isPending) body = <ListSkeleton />;
  else if (!data) {
    // A server without requests answers 404 (an early one 403): as good as off.
    body =
      isApiError(summary.error, 404) || isApiError(summary.error, 403) ? (
        <RequestsUnavailable reason="off" admin={me.is_admin} />
      ) : (
        <ProblemState error={summary.error} manage={me.is_admin} title="Couldn't load the requests" onRetry={() => void summary.refetch()} />
      );
  } else if (!data.enabled) body = <RequestsUnavailable reason="off" admin={me.is_admin} />;
  else if (data.sign_in_again === "requests") body = <ProblemState problem={{ kind: "sign_in" }} manage={false} />;
  else if (!hasRequests(data) && !data.manage) body = <RequestsUnavailable reason="no_access" admin={me.is_admin} />;
  else body = <RequestsView summary={data} />;

  const manage = data?.manage ?? false;
  return (
    <div className="space-y-5 px-4 pb-16 pt-6 sm:px-8 sm:pt-8">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h1 className="text-2xl font-bold tracking-tight">{manage ? "Requests" : "Your requests"}</h1>
          <p className="mt-1 text-sm text-muted">
            {manage
              ? "What people asked for. Approving sends a title to Radarr or Sonarr where they're connected."
              : "What you asked for and follow, and how it's going."}
          </p>
        </div>
        {mayRequest(data) ? (
          <Link to={discoverUrl()} className={buttonClass("outline", "sm")}>
            <Compass className="size-4" />
            Discover
          </Link>
        ) : null}
      </div>
      {data?.sign_in_again === "approvals" ? (
        <div className="flex flex-wrap items-center gap-3 rounded-lg border border-border bg-surface px-4 py-3 text-sm">
          <span className="flex-1">Sign in again to approve and decline here.</span>
          <SignInAgain size="sm" />
        </div>
      ) : null}
      {body}
    </div>
  );
}
