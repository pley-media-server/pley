import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { CircleStop, MonitorSmartphone, Pause, Play, RotateCcw, RotateCw, SkipBack, SkipForward } from "lucide-react";
import { queries, sessionCommandError, usePlayingSessions, useSessionCommand, useStopSession, type SessionCommand } from "@/api/hooks";
import type { Control } from "@/api/types";
import { cn } from "@/lib/cn";
import { formatClock, fullTitle, pluralize } from "@/lib/format";
import { useDismiss, useNow } from "@/lib/hooks";
import { posterCandidates } from "@/lib/images";
import { useSessionsHere } from "@/lib/playingHere";
import {
  ANSWER_WAIT_MS,
  commandAnswered,
  estimatedPosition,
  SKIP_BACK_MS,
  SKIP_FORWARD_MS,
  type RemoteSession,
  type SentCommand,
} from "@/lib/remote";
import { Artwork } from "./Artwork";
import { ChannelLogo } from "./ChannelLogo";
import { Slider } from "./Slider";
import { IconButton, ProgressBar } from "./ui";

/** What a remote calls a device the server couldn't name. */
const UNNAMED = "Unknown device";

/** The user's sessions that play somewhere other than this tab: what its remote controls. */
function useOtherSessions(): RemoteSession[] {
  const sessions = usePlayingSessions();
  const here = useSessionsHere();
  return (sessions.data ?? []).filter((s) => !here.has(s.session_id));
}

/**
 * What the remote calls each session's device. Two of the same name (two
 * tabs of one browser, two of the same TV) are numbered, so every label and
 * message says which one it means.
 */
function deviceNames(sessions: RemoteSession[]): Map<string, string> {
  const total = new Map<string, number>();
  for (const s of sessions) {
    const name = s.device ?? UNNAMED;
    total.set(name, (total.get(name) ?? 0) + 1);
  }
  const seen = new Map<string, number>();
  const names = new Map<string, string>();
  for (const s of sessions) {
    const name = s.device ?? UNNAMED;
    const n = (seen.get(name) ?? 0) + 1;
    seen.set(name, n);
    names.set(s.session_id, (total.get(name) ?? 0) > 1 ? `${name} (${n})` : name);
  }
  return names;
}

/**
 * In the header while the user plays something on another of their devices:
 * it opens a remote for each, with what plays there and the commands its
 * player obeys.
 */
export function RemoteButton() {
  const others = useOtherSessions();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  // Focus was in the panel last: a control that goes with its row (Stop, a
  // session ending) hands it back to the button rather than to the page.
  const focusInPanel = useRef(false);
  const shown = open && others.length > 0;

  useDismiss(ref, shown, () => {
    if (panelRef.current?.contains(document.activeElement)) buttonRef.current?.focus();
    setOpen(false);
  });
  useLayoutEffect(() => {
    if (!shown || !focusInPanel.current) return;
    if (!panelRef.current?.contains(document.activeElement)) buttonRef.current?.focus();
  });

  // Nothing left to control: it opens closed when something plays again.
  if (!others.length && open) setOpen(false);
  if (!others.length) return null;

  const names = deviceNames(others);

  const label = `Remote, ${pluralize(others.length, "device")} playing`;
  return (
    <div ref={ref} className="relative">
      <button
        ref={buttonRef}
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-label={label}
        aria-expanded={open}
        title={label}
        className={cn(
          "flex size-9 items-center justify-center rounded-full transition-colors hover:bg-white/5 hover:text-fg",
          open ? "text-accent" : "text-muted",
        )}
      >
        <MonitorSmartphone className="size-5" />
      </button>
      {shown ? (
        // On a phone the button isn't at the screen's edge: the panel spans the screen instead.
        <div
          ref={panelRef}
          role="dialog"
          aria-label="Playing on your devices"
          onFocus={() => (focusInPanel.current = true)}
          onBlur={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget)) focusInPanel.current = false;
          }}
          className="fixed inset-x-4 top-16 z-40 max-h-[calc(100dvh-5rem)] animate-pop-in divide-y divide-border overflow-y-auto rounded-lg border border-border bg-surface-2 shadow-xl shadow-black/50 sm:absolute sm:inset-x-auto sm:right-0 sm:top-full sm:mt-2 sm:w-96"
        >
          {others.map((s) => (
            <RemoteRow key={s.session_id} session={s} device={names.get(s.session_id) ?? UNNAMED} />
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** One device in the remote: what plays there, how far, and the controls its player listed. */
function RemoteRow({ session, device }: { session: RemoteSession; device: string }) {
  const qc = useQueryClient();
  const command = useSessionCommand();
  const stop = useStopSession();
  const nowMs = useNow(1_000) * 1000;
  const [sent, setSent] = useState<SentCommand | null>(null);
  const [error, setError] = useState<string | null>(null);
  const can = (c: Control) => session.controls.includes(c);

  // The list shows the change (the player's report brings `sessions_changed`): it answered.
  if (sent && commandAnswered(sent, session, nowMs)) setSent(null);

  // Not yet after a while: ask the server once more before saying so.
  useEffect(() => {
    if (!sent) return;
    let current = true;
    const timer = window.setTimeout(() => {
      void qc
        .fetchQuery({ ...queries.sessions(), staleTime: 0 })
        .catch(() => undefined)
        .then((list) => {
          if (!current) return;
          setSent(null);
          // A player with nothing next or before stays where it is, as it should.
          if (sent.command === "next" || sent.command === "previous") return;
          if (list && !commandAnswered(sent, list.find((s) => s.session_id === sent.before.session_id), Date.now())) {
            setError(`${device} didn't answer.`);
          }
        });
    }, ANSWER_WAIT_MS);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [sent, qc, device]);

  const position = estimatedPosition(session, nowMs);
  const duration = session.duration_ms;

  const send = (c: SessionCommand["command"], extra: Pick<SessionCommand, "positionMs" | "offsetMs"> = {}) => {
    setError(null);
    const sentAt = Date.now();
    const at = estimatedPosition(session, sentAt);
    // A previous a few seconds in starts the title over, as players do.
    const target =
      extra.positionMs ??
      (extra.offsetMs != null
        ? Math.max(0, Math.min(at + extra.offsetMs, duration ?? Number.MAX_SAFE_INTEGER))
        : c === "previous" && session.kind !== "live"
          ? 0
          : undefined);
    command.mutate(
      { sessionId: session.session_id, command: c, ...extra },
      {
        onSuccess: () => setSent({ command: c, before: session, sentAt, target }),
        onError: (err) => setError(`${sessionCommandError(err, device)}.`),
      },
    );
  };

  const paused = session.state === "paused";
  const toggle = paused ? (can("play") ? "play" : null) : can("pause") ? "pause" : null;
  const live = session.kind === "live";
  const title = live ? (session.channel?.name ?? "Live TV") : session.item ? fullTitle(session.item) : "";
  const waiting = command.isPending || !!sent;

  return (
    <div role="group" aria-label={device} aria-busy={waiting} className="space-y-2 px-4 py-3">
      <div className="flex items-center gap-3">
        {live && session.channel ? (
          <ChannelLogo channel={session.channel} width={64} showName={false} className="h-10 w-14 shrink-0 rounded bg-surface-3" />
        ) : session.item ? (
          <Artwork
            candidates={posterCandidates(session.item)}
            width={40}
            alt=""
            title={session.item.title}
            className={cn("w-10 shrink-0 rounded", session.item.kind === "track" ? "aspect-square" : "aspect-[2/3]")}
          />
        ) : null}
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs font-medium text-subtle">{device}</div>
          <div className="line-clamp-2 text-sm font-semibold text-fg" title={title}>
            {title}
          </div>
          {live && session.channel?.now ? <div className="truncate text-xs text-muted">{session.channel.now}</div> : null}
        </div>
      </div>

      {!live && duration ? (
        <div className="flex items-center gap-2 text-[11px] tabular-nums text-subtle">
          <span className="w-11 text-right">{formatClock(position / 1000)}</span>
          {can("seek") ? (
            <Slider
              label={`Seek on ${device}`}
              value={position}
              max={duration}
              step={1_000}
              valueText={(ms) => `${formatClock(ms / 1000)} of ${formatClock(duration / 1000)}`}
              onCommit={(ms) => send("seek", { positionMs: Math.round(ms) })}
              className="flex-1"
            />
          ) : (
            <ProgressBar value={position / duration} className="h-1 flex-1" label={`Progress on ${device}`} />
          )}
          <span className="w-11">{formatClock(duration / 1000)}</span>
        </div>
      ) : null}

      <div className="flex items-center gap-1">
        <div className={cn("flex items-center gap-1 transition-opacity", waiting && "opacity-50")}>
          {can("previous") ? (
            <IconButton label={live ? `Previous channel on ${device}` : `Previous on ${device}`} size="sm" onClick={() => send("previous")}>
              <SkipBack className="size-4 fill-current" />
            </IconButton>
          ) : null}
          {can("skip") ? (
            <IconButton label={`Back 10 seconds on ${device}`} size="sm" onClick={() => send("skip", { offsetMs: -SKIP_BACK_MS })}>
              <RotateCcw className="size-4" />
            </IconButton>
          ) : null}
          {toggle ? (
            <button
              type="button"
              aria-label={`${toggle === "play" ? "Play" : "Pause"} on ${device}`}
              title={toggle === "play" ? "Play" : "Pause"}
              onClick={() => send(toggle)}
              className="flex size-8 items-center justify-center rounded-full bg-fg text-bg transition hover:scale-105"
            >
              {toggle === "play" ? <Play className="ml-0.5 size-4 fill-current" /> : <Pause className="size-4 fill-current" />}
            </button>
          ) : null}
          {can("skip") ? (
            <IconButton label={`Forward 30 seconds on ${device}`} size="sm" onClick={() => send("skip", { offsetMs: SKIP_FORWARD_MS })}>
              <RotateCw className="size-4" />
            </IconButton>
          ) : null}
          {can("next") ? (
            <IconButton label={live ? `Next channel on ${device}` : `Next on ${device}`} size="sm" onClick={() => send("next")}>
              <SkipForward className="size-4 fill-current" />
            </IconButton>
          ) : null}
        </div>
        <span className="sr-only" role="status">
          {waiting ? `Waiting for ${device}` : ""}
        </span>
        <button
          type="button"
          onClick={() => {
            setError(null);
            stop.mutate(
              { sessionId: session.session_id },
              { onError: (err) => setError(`${sessionCommandError(err, device)}.`) },
            );
          }}
          disabled={stop.isPending}
          aria-label={`Stop on ${device}`}
          className="ml-auto flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-muted transition-colors hover:bg-danger/10 hover:text-danger disabled:opacity-50"
        >
          <CircleStop className="size-3.5" />
          Stop
        </button>
      </div>

      {error ? (
        <p role="alert" className="text-xs text-danger">
          {error}
        </p>
      ) : null}
    </div>
  );
}
