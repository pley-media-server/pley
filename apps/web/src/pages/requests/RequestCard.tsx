import { useRef, useState, type ReactNode } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Ban, Check, Ellipsis, Hand, Play, Plug, RotateCw, Settings2, Trash2, TriangleAlert, Undo2, UserMinus } from "lucide-react";
import { errorMessage } from "@/api/client";
import {
  requestProblem,
  useAskRequest,
  useApprovalOptions,
  useApproveRequest,
  useDeclineRequest,
  useDeleteRequest,
  useHandledRequest,
  useUnwatchRequest,
  type Approval,
  type ArrChoices,
  type MediaRequest,
  type RequestAction,
  type RequestSummary,
  type TitleKind,
} from "@/api/requests";
import { Artwork } from "@/components/Artwork";
import { Avatar } from "@/components/Avatar";
import { Menu, MenuItem } from "@/components/Menu";
import { ConfirmModal, Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button, buttonClass, Field, IconButton, Select, Skeleton, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";
import { formatAgo, formatBytes, formatTimestamp } from "@/lib/format";
import { useOnChange } from "@/lib/hooks";
import { actionLabel, actionQuestion, askedText, decidedText, kindLabel, problemText, seasonsLabel } from "@/lib/requests";
import { itemUrl, titleUrl } from "@/lib/urls";
import { StagePill } from "./shared";

/** Where an approved request of this kind goes. */
function service(request: Pick<MediaRequest, "kind">): "Radarr" | "Sonarr" {
  return request.kind === "movie" ? "Radarr" : "Sonarr";
}

function When({ ts }: { ts: number }) {
  return (
    <time dateTime={new Date(ts * 1000).toISOString()} title={formatTimestamp(ts)}>
      {formatAgo(ts)}
    </time>
  );
}

/** A failed answer, as a notice says it (with the limits, where asking went past one). */
export function failure(
  err: unknown,
  summary: Pick<RequestSummary, "manage"> & Partial<Pick<RequestSummary, "movies_limit" | "seasons_limit">>,
  kind: TitleKind,
): string {
  const problem = requestProblem(err);
  if (!problem) return errorMessage(err);
  const limits = { movies_limit: summary.movies_limit ?? null, seasons_limit: summary.seasons_limit ?? null };
  return problemText(problem, { manage: summary.manage, kind, limits });
}

function Checkbox({ checked }: { checked: boolean }) {
  return (
    <span
      aria-hidden
      className={cn(
        "flex size-4 shrink-0 items-center justify-center rounded border transition-colors peer-focus-visible:ring-2 peer-focus-visible:ring-accent",
        checked ? "border-accent bg-accent text-accent-fg" : "border-border-strong",
      )}
    >
      {checked ? <Check className="size-3" strokeWidth={3} /> : null}
    </span>
  );
}

/** A labelled checkbox that looks like the app's. */
export function CheckRow({
  id,
  checked,
  disabled,
  onChange,
  children,
  aside,
}: {
  id: string;
  checked: boolean;
  disabled?: boolean;
  onChange: () => void;
  children: ReactNode;
  aside?: ReactNode;
}) {
  return (
    <label
      htmlFor={id}
      className={cn("flex items-center gap-3 rounded px-2 py-1.5 text-sm", disabled ? "opacity-55" : "cursor-pointer hover:bg-white/5")}
    >
      <input id={id} type="checkbox" className="peer sr-only" checked={checked} disabled={disabled} onChange={onChange} />
      <Checkbox checked={checked} />
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {aside ? <span className="shrink-0 text-xs text-subtle">{aside}</span> : null}
    </label>
  );
}

/** Why the approve dialog has less to choose from than it might, in a box of its own. */
function ServiceNote({ icon, title, children }: { icon: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className="flex gap-3 rounded-md border border-border bg-surface-2/60 px-3 py-2.5 text-sm">
      <span className="mt-0.5 shrink-0 [&>svg]:size-4">{icon}</span>
      <div className="min-w-0 space-y-0.5">
        <p className="font-medium text-fg">{title}</p>
        {children ? <p className="text-muted">{children}</p> : null}
      </div>
    </div>
  );
}

/** The profile and folder a request is preselected with: anime's for anime, else the settings' own. */
function preselected(choices: ArrChoices, anime: boolean): { profile: number | null; folder: string | null } {
  return {
    profile: (anime ? choices.anime_quality_profile : null) ?? choices.quality_profile,
    folder: (anime ? choices.anime_root_folder : null) ?? choices.root_folder,
  };
}

/**
 * Approving, or trying a failed request again (`action`), with choices of
 * its own: Radarr's or Sonarr's profile and folder (the settings' ones
 * preselected, anime's for anime), and for a show which of the seasons
 * asked for to keep. A service that didn't answer, with nothing known of
 * it, leaves only the settings' choices.
 */
export function ApproveDialog({
  request,
  action = "approve",
  open,
  onClose,
}: {
  request: MediaRequest;
  action?: "approve" | "retry";
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation(["requests", "common"]);
  const options = useApprovalOptions(open);
  const approve = useApproveRequest();
  const toast = useToast();
  const choices = options.data ? (request.kind === "movie" ? options.data.radarr : options.data.sonarr) : undefined;
  const lists = !!choices && choices.quality_profiles.length > 0 && choices.root_folders.length > 0;
  // Connected, with nothing to choose from (or nothing known of it): approving sends the settings' choices.
  const settingsOnly = !options.isPending && choices !== null && !lists;
  const asked = request.seasons ?? [];
  const pick = choices ? preselected(choices, request.anime) : { profile: null, folder: null };
  // What was picked instead of the preselected ones (`null`: nothing).
  const [chosen, setChosen] = useState<{ profile: number | null; folder: string | null }>({ profile: null, folder: null });
  const profile = chosen.profile ?? pick.profile;
  const folder = chosen.folder ?? pick.folder;
  // The settings' choice where Radarr or Sonarr no longer lists it: it stays selected, as gone, since it is
  // what approving sends unless another is picked.
  const goneProfile = profile !== null && !choices?.quality_profiles.some((p) => p.id === profile) ? profile : null;
  const goneFolder = folder !== null && !choices?.root_folders.some((f) => f.path === folder) ? folder : null;
  const [seasons, setSeasons] = useState<number[]>(asked);
  const [error, setError] = useState<string | null>(null);

  useOnChange(open, (o) => {
    if (!o) return;
    setChosen({ profile: null, folder: null });
    setSeasons(request.seasons ?? []);
    setError(null);
  });

  const submit = () => {
    if (asked.length && !seasons.length) return setError(t("shared.pickSeason"));
    const body: Approval = {};
    if (lists) {
      if (profile !== null && profile !== pick.profile) body.quality_profile = profile;
      if (folder !== null && folder !== pick.folder) body.root_folder = folder;
    }
    if (asked.length && seasons.length < asked.length) body.seasons = [...seasons].sort((a, b) => a - b);
    setError(null);
    approve.mutate(
      { request, ...body },
      {
        onSuccess: (r) => {
          toast.success(decidedText(r.title, "approved", null));
          onClose();
        },
        onError: (e) => setError(failure(e, { manage: true }, request.kind)),
      },
    );
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      locked={approve.isPending}
      size="sm"
      title={t(action === "retry" ? "approve.retryTitle" : "approve.title", { title: request.title })}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={approve.isPending}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" onClick={submit} loading={approve.isPending} data-autofocus>
            {settingsOnly ? t(action === "retry" ? "approve.retryWithSettings" : "approve.withSettings") : actionLabel(action, request)}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {options.isPending ? (
          <div className="space-y-3" aria-busy>
            <Skeleton className="h-14" />
            <Skeleton className="h-14" />
          </div>
        ) : choices === null ? (
          <ServiceNote icon={<Plug className="text-subtle" />} title={t("approve.notConnected", { service: service(request) })}>
            {t("approve.notConnectedText")}
          </ServiceNote>
        ) : choices ? (
          <>
            {choices.error ? (
              <ServiceNote icon={<TriangleAlert className="text-warning" />} title={choices.error}>
                {lists ? t("approve.lastOptions") : null}
              </ServiceNote>
            ) : null}
            {lists ? (
              <>
                <Field label={t("approve.qualityProfile")} htmlFor={`approve-profile-${request.id}`}>
                  <Select
                    id={`approve-profile-${request.id}`}
                    className="w-full"
                    value={profile ?? ""}
                    onChange={(e) => setChosen((c) => ({ ...c, profile: Number(e.target.value) }))}
                  >
                    {profile === null ? <option value="">{t("approve.settingsChoice")}</option> : null}
                    {goneProfile !== null ? <option value={goneProfile}>{t("approve.goneProfile", { id: goneProfile })}</option> : null}
                    {choices.quality_profiles.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field label={t("approve.folder")} htmlFor={`approve-folder-${request.id}`}>
                  <Select
                    id={`approve-folder-${request.id}`}
                    className="w-full"
                    value={folder ?? ""}
                    onChange={(e) => setChosen((c) => ({ ...c, folder: e.target.value }))}
                  >
                    {folder === null ? <option value="">{t("approve.settingsChoice")}</option> : null}
                    {goneFolder !== null ? <option value={goneFolder}>{t("approve.goneFolder", { folder: goneFolder })}</option> : null}
                    {choices.root_folders.map((f) => (
                      <option key={f.path} value={f.path}>
                        {f.free_bytes !== null ? t("approve.freeSpace", { path: f.path, free: formatBytes(f.free_bytes) }) : f.path}
                      </option>
                    ))}
                  </Select>
                </Field>
              </>
            ) : null}
          </>
        ) : (
          // The options couldn't be had at all: the settings' choices still can.
          <ServiceNote icon={<TriangleAlert className="text-warning" />} title={t("approve.optionsFailed", { service: service(request) })} />
        )}
        {asked.length > 1 ? (
          <fieldset className="space-y-1">
            <legend className="mb-1 text-sm font-medium">{t("shared.seasons")}</legend>
            <p className="pb-1 text-xs text-subtle">{t("approve.seasonsHint")}</p>
            <div className="max-h-56 space-y-0.5 overflow-y-auto rounded-md border border-border p-1.5">
              {[...asked]
                .sort((a, b) => Number(a === 0) - Number(b === 0) || a - b)
                .map((n) => (
                  <CheckRow
                    key={n}
                    id={`approve-${request.id}-season-${n}`}
                    checked={seasons.includes(n)}
                    onChange={() => setSeasons((s) => (s.includes(n) ? s.filter((x) => x !== n) : [...s, n]))}
                  >
                    {n === 0 ? t("common:format.specials") : t("common:format.seasonNumber", { number: n })}
                  </CheckRow>
                ))}
            </div>
          </fieldset>
        ) : null}
        {error ? (
          <p role="alert" className="text-sm text-danger">
            {error}
          </p>
        ) : null}
      </div>
    </Modal>
  );
}

/** Declining, with a reason for the person who asked if there is one. */
export function DeclineDialog({ request, open, onClose }: { request: MediaRequest; open: boolean; onClose: () => void }) {
  const { t } = useTranslation(["requests", "common"]);
  const decline = useDeclineRequest();
  const toast = useToast();
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  // Set at once (`isPending` follows a moment later), so that Enter pressed twice declines once: a second
  // decline is refused, and would say the request changed meanwhile.
  const sending = useRef(false);
  useOnChange(open, (o) => {
    if (!o) return;
    setReason("");
    setError(null);
  });
  const submit = () => {
    if (sending.current) return;
    sending.current = true;
    decline.mutate(
      { request, reason: reason.trim() || undefined },
      {
        onSuccess: (r) => {
          toast.success(decidedText(r.title, "declined", null));
          onClose();
        },
        onError: (e) => setError(failure(e, { manage: true }, request.kind)),
        onSettled: () => {
          sending.current = false;
        },
      },
    );
  };
  const who = request.requested_by?.name;
  return (
    <Modal
      open={open}
      onClose={onClose}
      locked={decline.isPending}
      size="sm"
      title={t("decline.title", { title: request.title })}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={decline.isPending}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="danger" onClick={submit} loading={decline.isPending}>
            {t("shared.decline")}
          </Button>
        </>
      }
    >
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <p className="text-sm text-muted">
          {who ? t("decline.nameHears", { name: who }) : t("decline.someoneHears")}
          {request.watchers > 0 ? ` ${t("decline.followersHear")}` : ""}
        </p>
        <Field label={t("shared.reason")} htmlFor={`decline-${request.id}`} hint={t("decline.optional")}>
          <TextInput
            id={`decline-${request.id}`}
            maxLength={500}
            autoComplete="off"
            placeholder={t("decline.placeholder")}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            data-autofocus
          />
        </Field>
        {error ? (
          <p role="alert" className="text-sm text-danger">
            {error}
          </p>
        ) : null}
        <button type="submit" className="hidden" />
      </form>
    </Modal>
  );
}

const ACTION_ICONS: Record<RequestAction, ReactNode> = {
  withdraw: <Undo2 className="size-4" />,
  unwatch: <UserMinus className="size-4" />,
  approve: <Check className="size-4" />,
  retry: <RotateCw className="size-3.5" />,
  decline: <Ban className="size-3.5" />,
  handled: <Hand className="size-4" />,
  delete: <Trash2 className="size-4" />,
  ask_again: <RotateCw className="size-3.5" />,
};

/** Actions that go in the card's menu rather than as buttons: those seldom wanted. */
const IN_MENU: RequestAction[] = ["handled", "delete"];

/**
 * What this session may do to a request, as the server lists them, in its
 * order (but those in `skip`, which the page offers otherwise): approving
 * or trying again with the settings' choices, or with choices of its own
 * from the menu, declining with a reason, withdrawing and deleting after
 * asking.
 */
export function RequestActions({ request, summary, skip = [] }: { request: MediaRequest; summary: RequestSummary; skip?: RequestAction[] }) {
  const { t } = useTranslation(["requests", "common"]);
  const toast = useToast();
  const approve = useApproveRequest();
  const handled = useHandledRequest();
  const remove = useDeleteRequest();
  const unwatch = useUnwatchRequest();
  const ask = useAskRequest();
  const [dialog, setDialog] = useState<"approve" | "decline" | "withdraw" | "delete" | null>(null);
  const busy = approve.isPending || handled.isPending || remove.isPending || unwatch.isPending || ask.isPending;
  const fail = (e: unknown) => toast.error(failure(e, summary, request.kind));

  const run = (action: RequestAction) => {
    switch (action) {
      case "approve":
      case "retry":
        return approve.mutate({ request }, { onSuccess: (r) => toast.success(decidedText(r.title, "approved", null)), onError: fail });
      case "decline":
        return setDialog("decline");
      case "handled":
        return handled.mutate(request, { onSuccess: (r) => toast.success(t("actions.markedByHand", { title: r.title })), onError: fail });
      case "withdraw":
        return setDialog("withdraw");
      case "delete":
        return setDialog("delete");
      case "unwatch":
        return unwatch.mutate(request, { onSuccess: () => toast.success(t("actions.unfollowed", { title: request.title })), onError: fail });
      case "ask_again":
        return ask.mutate(
          { kind: request.kind, tmdb_id: request.tmdb_id, ...(request.seasons ? { seasons: request.seasons } : {}) },
          { onSuccess: (asked) => toast.success(askedText(asked)), onError: fail },
        );
    }
  };

  const actions = request.actions.filter((a) => !skip.includes(a));
  const buttons = actions.filter((a) => !IN_MENU.includes(a));
  const menu = actions.filter((a) => IN_MENU.includes(a));
  // Approving, or trying a failed one again, with choices of its own: from the menu.
  const choosing = actions.find((a): a is "approve" | "retry" => a === "approve" || a === "retry") ?? null;
  const question = dialog === "withdraw" || dialog === "delete" ? actionQuestion(dialog, request) : null;

  if (!actions.length) return null;
  return (
    <div className="flex flex-wrap items-center justify-end gap-2">
      {buttons.map((action) => (
        <Button
          key={action}
          variant={action === "approve" ? "primary" : action === "retry" || action === "ask_again" || action === "decline" ? "outline" : "ghost"}
          size="sm"
          icon={ACTION_ICONS[action]}
          loading={
            ((action === "approve" || action === "retry") && approve.isPending) || (action === "ask_again" && ask.isPending) || (action === "unwatch" && unwatch.isPending)
          }
          disabled={busy}
          onClick={() => run(action)}
        >
          {actionLabel(action, request)}
        </Button>
      ))}
      {menu.length || choosing ? (
        <Menu
          className="w-max"
          trigger={({ open, toggle }) => (
            <IconButton size="sm" label={t("actions.more", { title: request.title })} aria-haspopup="menu" aria-expanded={open} onClick={toggle} disabled={busy}>
              <Ellipsis className="size-4" />
            </IconButton>
          )}
        >
          {(close) => (
            <>
              {choosing ? (
                <MenuItem
                  icon={<Settings2 />}
                  onClick={() => {
                    close();
                    setDialog("approve");
                  }}
                >
                  {actionLabel(choosing, request)}…
                </MenuItem>
              ) : null}
              {menu.map((action) => (
                <MenuItem
                  key={action}
                  icon={ACTION_ICONS[action]}
                  danger={action === "delete"}
                  onClick={() => {
                    close();
                    run(action);
                  }}
                >
                  {actionLabel(action, request)}
                </MenuItem>
              ))}
            </>
          )}
        </Menu>
      ) : null}

      {choosing ? <ApproveDialog request={request} action={choosing} open={dialog === "approve"} onClose={() => setDialog(null)} /> : null}
      {actions.includes("decline") ? <DeclineDialog request={request} open={dialog === "decline"} onClose={() => setDialog(null)} /> : null}
      <ConfirmModal
        open={question !== null}
        onClose={() => setDialog(null)}
        title={question ?? ""}
        danger
        confirmLabel={dialog === "delete" ? t("common:actions.delete") : actionLabel("withdraw", request)}
        loading={remove.isPending}
        message={
          dialog === "delete" ? (
            <Trans t={t} i18nKey="actions.deleteText" values={{ title: request.title }} components={{ strong: <strong className="text-fg" /> }} />
          ) : request.status === "declined" ? (
            t("actions.askAgain")
          ) : (
            t(request.watchers > 0 ? "actions.noMoreUnlessFollowed" : "actions.noMore")
          )
        }
        onConfirm={() =>
          remove.mutate(request, {
            onSuccess: () => setDialog(null),
            onError: (e) => {
              setDialog(null);
              fail(e);
            },
          })
        }
      />
    </div>
  );
}

/** Who asked, for managers (or "Asked for by someone else"), and when. */
function Asker({ request, manage }: { request: MediaRequest; manage: boolean }) {
  const { t } = useTranslation("requests");
  const by = request.mine ? null : request.requested_by;
  const when = <When ts={request.created_at} />;
  return (
    <div className="flex items-center gap-1.5 text-xs text-subtle">
      {by ? <Avatar user={by} size="xs" /> : null}
      {/* One block of words, so a long name wraps them rather than leaving the picture alone on a line. */}
      <p className="min-w-0">
        {request.mine ? (
          <Trans t={t} i18nKey="card.youAsked" components={{ when }} />
        ) : by ? (
          <Trans t={t} i18nKey="card.nameAsked" values={{ name: by.name }} components={{ name: <span className="text-muted" />, when }} />
        ) : (
          <Trans t={t} i18nKey="card.someoneAsked" components={{ when }} />
        )}
        {request.watching && !request.mine ? ` · ${t("card.youFollow")}` : null}
        {manage && request.watchers > 0 ? ` · ${t("card.followers", { count: request.watchers })}` : null}
      </p>
    </div>
  );
}

/** What there is to know beyond its stage: why it was declined, the managers' note, and where it was sent. */
function Notes({ request, manage }: { request: MediaRequest; manage: boolean }) {
  const { t } = useTranslation("requests");
  const lines: ReactNode[] = [];
  if (request.status === "declined") {
    const by = request.decided_by && manage ? request.decided_by.name : null;
    lines.push(
      <span key="d">
        {request.decided_at ? (
          <Trans
            t={t}
            i18nKey={by ? "card.declinedByWhen" : "card.declinedWhen"}
            values={{ name: by }}
            components={{ when: <When ts={request.decided_at} /> }}
          />
        ) : by ? (
          t("card.declinedBy", { name: by })
        ) : (
          t("card.declined")
        )}
        {request.reason ? <span className="text-fg/85">{t("card.reason", { reason: request.reason })}</span> : null}
      </span>,
    );
  }
  if (manage && request.note) lines.push(<span key="n">{request.note}</span>);
  if (manage && request.sent_to) {
    const where = [request.sent_to.service === "radarr" ? "Radarr" : "Sonarr", request.sent_to.quality_profile, request.sent_to.root_folder].filter(Boolean);
    lines.push(<span key="s">{t("card.sentTo", { where: where.join(" · ") })}</span>);
  }
  if (!lines.length) return null;
  return (
    <div className="space-y-0.5 text-xs text-muted">
      {lines.map((line, i) => (
        <p key={i}>{line}</p>
      ))}
    </div>
  );
}

function isReady(request: MediaRequest): boolean {
  return request.stage === "available" || request.stage === "partial";
}

/** Ready, in a library this account can't open: who can help (managers know). */
function OutOfReach({ request, manage }: { request: MediaRequest; manage: boolean }) {
  const { t } = useTranslation("requests");
  if (manage || !isReady(request) || request.item_id !== null) return null;
  return <p className="text-xs text-muted">{t("card.outOfReach")}</p>;
}

/**
 * A request in a list: its poster and title (unless the list is on the
 * title's page, which offers Open and Request itself), its seasons, a stage
 * pill with a download's progress, who asked and when, the reason or note,
 * and what this session may do. `onSelect` puts a checkbox before it, for
 * deciding on several at once; `checkboxes`: the list has them, so one
 * without keeps their room.
 */
export function RequestCard({
  request,
  summary,
  showTitle = true,
  selected,
  onSelect,
  checkboxes,
}: {
  request: MediaRequest;
  summary: RequestSummary;
  showTitle?: boolean;
  selected?: boolean;
  onSelect?: (selected: boolean) => void;
  checkboxes?: boolean;
}) {
  const { t } = useTranslation("requests");
  const manage = summary.manage;
  const seasons = seasonsLabel(request.seasons);
  const to = titleUrl(request.kind, request.tmdb_id);
  const skip: RequestAction[] = showTitle ? [] : ["ask_again"];
  // Ready, where this account can open it: Watch.
  const watch = showTitle && isReady(request) ? request.item_id : null;
  const actionable = watch !== null || request.actions.some((a) => !skip.includes(a));
  return (
    <li className={cn("flex flex-col gap-3 px-4 py-3.5 first:rounded-t-lg last:rounded-b-lg md:flex-row md:items-start", request.unseen && "bg-accent/[0.04]")}>
      <div className="flex min-w-0 flex-1 gap-3">
        {onSelect ? (
          <label className="flex shrink-0 cursor-pointer items-start pt-1">
            <input
              type="checkbox"
              className="peer sr-only"
              checked={!!selected}
              onChange={(e) => onSelect(e.target.checked)}
              aria-label={t("card.select", { title: request.title })}
            />
            <Checkbox checked={!!selected} />
          </label>
        ) : checkboxes ? (
          <span className="w-4 shrink-0" />
        ) : null}
        {showTitle ? (
          <Link to={to} className="shrink-0" tabIndex={-1} aria-hidden>
            <Artwork
              candidates={request.poster_url ? [request.poster_url] : []}
              width={56}
              alt=""
              title=""
              bare
              className="aspect-[2/3] w-14 rounded ring-1 ring-white/5"
            />
          </Link>
        ) : null}
        <div className="min-w-0 flex-1 space-y-1.5">
          {showTitle || seasons || request.unseen ? (
            <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
              {showTitle ? (
                <Link to={to} className="truncate font-medium transition-colors hover:text-accent">
                  {request.title}
                </Link>
              ) : seasons ? (
                <span className="font-medium">{seasons}</span>
              ) : null}
              {showTitle && request.year ? <span className="text-sm text-subtle">{request.year}</span> : null}
              {showTitle ? (
                <span className="text-xs text-subtle">
                  {kindLabel(request.kind)}
                  {seasons ? ` · ${seasons}` : ""}
                </span>
              ) : null}
              {request.unseen ? (
                <span className="self-center rounded-full bg-accent/15 px-1.5 text-[10px] font-semibold uppercase leading-4 tracking-wider text-accent">{t("card.new")}</span>
              ) : null}
            </div>
          ) : null}
          <StagePill request={request} manage={manage} />
          <OutOfReach request={request} manage={manage} />
          <Asker request={request} manage={manage} />
          <Notes request={request} manage={manage} />
        </div>
      </div>
      {actionable ? (
        <div className="flex shrink-0 flex-wrap items-center justify-end gap-2 md:max-w-[55%]">
          {watch !== null ? (
            <Link to={itemUrl(watch)} className={buttonClass("secondary", "sm")}>
              <Play className="size-3.5 fill-current" />
              {t("card.watch")}
            </Link>
          ) : null}
          <RequestActions request={request} summary={summary} skip={skip} />
        </div>
      ) : null}
    </li>
  );
}
