import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  AppWindow,
  AudioLines,
  Captions,
  Check,
  ChevronDown,
  CircleStop,
  Laptop,
  MonitorSmartphone,
  Pause,
  Play,
  RotateCcw,
  RotateCw,
  SkipBack,
  SkipForward,
  Smartphone,
  Tablet,
  Tv,
} from "lucide-react";
import {
  queries,
  sessionCommandError,
  usePlayingSessions,
  useRemoteDevices,
  useSessionCommand,
  useStopSession,
  type SessionCommand,
} from "@/api/hooks";
import type { Control, DeviceKind } from "@/api/types";
import { cn } from "@/lib/cn";
import { episodeLabel, formatClock, fullTitle, pluralize } from "@/lib/format";
import { useDismiss, useNow } from "@/lib/hooks";
import { posterCandidates } from "@/lib/images";
import { isPlayingHere, useSessionsHere } from "@/lib/playingHere";
import {
  ANSWER_WAIT_MS,
  commandAnswered,
  estimatedPosition,
  newestOn,
  otherDevices,
  SKIP_BACK_MS,
  SKIP_FORWARD_MS,
  trackLabel,
  type OtherDevice,
  type RemoteSession,
  type SentCommand,
} from "@/lib/remote";
import { Artwork } from "./Artwork";
import { ChannelLogo } from "./ChannelLogo";
import { Slider } from "./Slider";
import { IconButton, ProgressBar } from "./ui";

/**
 * The user's other devices: those that take casts and those that play, this
 * tab's own sessions left out (a tab controls the user's other devices, not
 * itself). `known`: both lists have been asked for.
 */
export function useOtherDevices(): { devices: OtherDevice[]; known: boolean } {
  const sessions = usePlayingSessions();
  const receivers = useRemoteDevices();
  const here = useSessionsHere();
  const devices = otherDevices(receivers.data ?? [], (sessions.data ?? []).filter((s) => !here.has(s.session_id)));
  return { devices, known: !sessions.isPending && !receivers.isPending };
}

/** A device's icon, for its kind. */
export function DeviceIcon({ kind, className }: { kind: DeviceKind; className?: string }) {
  switch (kind) {
    case "tv":
      return <Tv aria-hidden className={className} />;
    case "phone":
      return <Smartphone aria-hidden className={className} />;
    case "tablet":
      return <Tablet aria-hidden className={className} />;
    case "computer":
      return <Laptop aria-hidden className={className} />;
    case "browser":
      return <AppWindow aria-hidden className={className} />;
    default:
      return <MonitorSmartphone aria-hidden className={className} />;
  }
}

/** What `device` plays, as its row says it: a title in full, a channel ("1 Channel One"), or "Ready to play". */
function playingLine(device: OtherDevice): string {
  const s = device.session;
  if (!s) return "Ready to play";
  if (s.kind === "live") return s.channel ? [s.channel.number, s.channel.name].filter(Boolean).join(" ") : "Live TV";
  return s.item ? fullTitle(s.item) : "";
}

/**
 * A device as the remote lists it: its icon, its name, and what it plays,
 * with how far for a title, or "Ready to play". `lineId` names the line, for
 * the control around it to be described by.
 */
export function DeviceSummary({ device, lineId, chosen }: { device: OtherDevice; lineId: string; chosen?: boolean }) {
  const nowMs = useNow(1_000) * 1000;
  const s = device.session;
  const progress = s?.kind === "item" && s.duration_ms ? estimatedPosition(s, nowMs) / s.duration_ms : null;
  return (
    <>
      <DeviceIcon kind={device.kind} className={cn("size-5 shrink-0", chosen ? "text-accent" : "text-muted")} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-medium text-fg">{device.name}</span>
        <span id={lineId} className="block truncate text-xs text-muted">
          {playingLine(device)}
        </span>
        {progress !== null ? <ProgressBar value={progress} className="mt-1 h-0.5" /> : null}
      </span>
    </>
  );
}

/**
 * In the header while another of the user's devices is on, playing or ready
 * to: it lists them, and shows the one chosen below the list, with what
 * plays there and the commands its player obeys.
 */
export function RemoteButton() {
  const { devices, known } = useOtherDevices();
  const [open, setOpen] = useState(false);
  // Kept while the panel is open; with one device, that one is shown.
  const [chosen, setChosen] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const headingId = useId();
  // Focus was in the panel last: a control that goes with what it shows
  // (Stop, a device going) hands it back to the button rather than to the page.
  const focusInPanel = useRef(false);
  const shown = open && devices.length > 0;

  const close = () => {
    setOpen(false);
    setChosen(null);
  };
  useDismiss(ref, shown, () => {
    if (panelRef.current?.contains(document.activeElement)) buttonRef.current?.focus();
    close();
  });
  useLayoutEffect(() => {
    if (!shown || !focusInPanel.current) return;
    if (!panelRef.current?.contains(document.activeElement)) buttonRef.current?.focus();
  });

  // No other device on: it opens closed when one is on again.
  if (known && !devices.length && open) close();
  if (!devices.length) return null;

  const current = devices.length === 1 ? devices[0] : devices.find((d) => d.id === chosen);
  const playing = devices.filter((d) => d.session).length;
  const label = playing
    ? `Remote, ${pluralize(playing, "device")} playing`
    : `Remote, ${pluralize(devices.length, "device")} ready to play`;
  return (
    <div ref={ref} className="relative">
      <button
        ref={buttonRef}
        type="button"
        onClick={() => (open ? close() : setOpen(true))}
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
          aria-labelledby={headingId}
          onFocus={() => (focusInPanel.current = true)}
          onBlur={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget)) focusInPanel.current = false;
          }}
          className="fixed inset-x-4 top-16 z-40 max-h-[calc(100dvh-5rem)] animate-pop-in overflow-y-auto rounded-lg border border-border bg-surface-2 shadow-xl shadow-black/50 sm:absolute sm:inset-x-auto sm:right-0 sm:top-full sm:mt-2 sm:w-96"
        >
          <div className="p-2">
            <h2 id={headingId} className="px-2 pb-1 pt-1 text-xs font-medium text-subtle">
              Your devices
            </h2>
            <ul>
              {devices.map((d) => (
                <li key={d.id}>
                  <DeviceRow device={d} chosen={d === current} onChoose={() => setChosen(d.id)} />
                </li>
              ))}
            </ul>
          </div>
          {current ? (
            <div className="border-t border-border">
              {current.session ? (
                <PlayingControls key={current.id} device={current} session={current.session} />
              ) : (
                <IdleDevice device={current} />
              )}
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

/** One device in the list: choosing it shows its controls below. */
function DeviceRow({ device, chosen, onChoose }: { device: OtherDevice; chosen: boolean; onChoose: () => void }) {
  const lineId = useId();
  return (
    <button
      type="button"
      aria-label={device.name}
      aria-describedby={lineId}
      aria-current={chosen || undefined}
      onClick={onChoose}
      className={cn(
        "flex w-full items-center gap-3 rounded-md px-2 py-2 text-left transition-colors hover:bg-white/5",
        chosen && "bg-white/[0.06]",
      )}
    >
      <DeviceSummary device={device} lineId={lineId} chosen={chosen} />
    </button>
  );
}

/** A device ready to play, that plays nothing: how to send it something. */
function IdleDevice({ device }: { device: OtherDevice }) {
  return (
    <div role="group" aria-label={device.name} className="flex flex-col items-center gap-1 px-6 py-5 text-center">
      <DeviceIcon kind={device.kind} className="mb-1 size-8 text-subtle" />
      <div className="text-sm font-semibold text-fg">{device.name}</div>
      <div className="text-sm text-muted">Nothing playing</div>
      <p className="mt-1 text-xs text-subtle [text-wrap:pretty]">
        Find something to watch or listen to, then choose Play on… and this device.
      </p>
    </div>
  );
}

/**
 * What a session plays, on two lines: an episode's show, season and number,
 * then its title; a track, then its artist; a channel's number and name,
 * then what's on.
 */
function titleLines(session: RemoteSession): [string, string | null] {
  if (session.kind === "live") {
    const c = session.channel;
    return [c ? [c.number, c.name].filter(Boolean).join(" ") : "Live TV", c?.now ?? null];
  }
  const item = session.item;
  if (!item) return ["", null];
  if (item.kind === "episode") return [[item.grandparent_title, episodeLabel(item)].filter(Boolean).join(" · "), item.title];
  if (item.kind === "track") return [item.title, item.grandparent_title ?? null];
  return [fullTitle(item), null];
}

type Picker = "audio" | "subtitles";

/** A playing device's controls: what plays, how far, and the commands its player listed. */
function PlayingControls({ device, session }: { device: OtherDevice; session: RemoteSession }) {
  const name = device.name;
  const qc = useQueryClient();
  const command = useSessionCommand();
  const stop = useStopSession();
  const nowMs = useNow(1_000) * 1000;
  const [sent, setSent] = useState<SentCommand | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picking, setPicking] = useState<Picker | null>(null);
  const pickers = useRef<Partial<Record<Picker, HTMLButtonElement | null>>>({});
  const can = (c: Control) => session.controls.includes(c);

  // The list shows the change (the player's report brings `sessions_changed`): it answered.
  if (sent && commandAnswered(sent, session, nowMs)) setSent(null);

  // Not yet after a while: ask the server once more before saying so. The
  // device's newest session is what it plays by then (a next episode, or a
  // track that took a new session, is another).
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
          if (list && !commandAnswered(sent, newestOn(list, sent.before.device_id, isPlayingHere), Date.now())) {
            setError(`${name} didn't answer.`);
          }
        });
    }, ANSWER_WAIT_MS);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [sent, qc, name]);

  const position = estimatedPosition(session, nowMs);
  const duration = session.duration_ms;

  const send = (c: Control, extra: Pick<SessionCommand, "positionMs" | "offsetMs" | "streamIndex"> = {}) => {
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
        onSuccess: () => setSent({ command: c, before: session, sentAt, target, track: extra.streamIndex }),
        onError: (err) => setError(`${sessionCommandError(err, name)}.`),
      },
    );
  };

  const paused = session.state === "paused";
  const toggle = paused ? (can("play") ? "play" : null) : can("pause") ? "pause" : null;
  const live = session.kind === "live";
  const kind = session.item?.kind;
  // Next is there for whatever has a next (an episode, a track, a queue's
  // entry, a channel), greyed out where the player doesn't take it.
  const nextShown = can("next") || live || kind === "episode" || kind === "track";
  const transport = can("previous") || can("skip") || !!toggle || nextShown;
  const [title, detail] = titleLines(session);
  const waiting = command.isPending || !!sent;

  // Audio where there is a choice of tracks; subtitles where there are any, with Off.
  const audio = session.tracks.audio;
  const subtitles = session.tracks.subtitles;
  const offered: Record<Picker, boolean> = {
    audio: can("audio") && audio.length > 1,
    subtitles: can("subtitles") && subtitles.length > 0,
  };
  const options =
    picking === "audio"
      ? audio.map((t) => ({ index: t.index, label: trackLabel(t, "audio") }))
      : [{ index: -1, label: "Off" }, ...subtitles.map((t) => ({ index: t.index, label: trackLabel(t, "subtitle") }))];
  const closePicker = () => {
    if (picking) pickers.current[picking]?.focus();
    setPicking(null);
  };
  const pickerLabel = (p: Picker) => `${p === "audio" ? "Audio" : "Subtitles"} on ${name}`;

  return (
    <div role="group" aria-label={name} aria-busy={waiting} className="space-y-3 px-4 py-3">
      <div className="flex items-center gap-3">
        {live && session.channel ? (
          <ChannelLogo channel={session.channel} width={64} showName={false} className="h-10 w-14 shrink-0 rounded bg-surface-3" />
        ) : session.item ? (
          <Artwork
            candidates={posterCandidates(session.item)}
            width={40}
            alt=""
            title={session.item.title}
            // Too small for the title's text, which the lines beside it say anyway.
            bare
            className={cn("w-10 shrink-0 rounded", session.item.kind === "track" ? "aspect-square" : "aspect-[2/3]")}
          />
        ) : null}
        <div className="min-w-0 flex-1">
          <div className="line-clamp-2 text-sm font-semibold text-fg" title={title}>
            {title}
          </div>
          {detail ? <div className="truncate text-xs text-muted">{detail}</div> : null}
        </div>
      </div>

      {!live && duration ? (
        <div className="flex items-center gap-2 text-[11px] tabular-nums text-subtle">
          <span className="w-11 text-right">{formatClock(position / 1000)}</span>
          {can("seek") ? (
            <Slider
              label={`Seek on ${name}`}
              value={position}
              max={duration}
              step={1_000}
              valueText={(ms) => `${formatClock(ms / 1000)} of ${formatClock(duration / 1000)}`}
              onCommit={(ms) => send("seek", { positionMs: Math.round(ms) })}
              className="flex-1"
            />
          ) : (
            <ProgressBar value={position / duration} className="h-1 flex-1" label={`Progress on ${name}`} />
          )}
          <span className="w-11">{formatClock(duration / 1000)}</span>
        </div>
      ) : null}

      {transport ? (
        <div className={cn("flex items-center justify-center gap-2 transition-opacity", waiting && "opacity-50")}>
          {can("previous") ? (
            <IconButton label={live ? `Previous channel on ${name}` : `Previous on ${name}`} size="sm" onClick={() => send("previous")}>
              <SkipBack className="size-4 fill-current" />
            </IconButton>
          ) : null}
          {can("skip") ? (
            <IconButton label={`Back 10 seconds on ${name}`} size="sm" onClick={() => send("skip", { offsetMs: -SKIP_BACK_MS })}>
              <RotateCcw className="size-4" />
            </IconButton>
          ) : null}
          {toggle ? (
            <button
              type="button"
              aria-label={`${toggle === "play" ? "Play" : "Pause"} on ${name}`}
              title={toggle === "play" ? "Play" : "Pause"}
              onClick={() => send(toggle)}
              className="flex size-10 items-center justify-center rounded-full bg-fg text-bg transition hover:scale-105"
            >
              {toggle === "play" ? <Play className="ml-0.5 size-5 fill-current" /> : <Pause className="size-5 fill-current" />}
            </button>
          ) : null}
          {can("skip") ? (
            <IconButton label={`Forward 30 seconds on ${name}`} size="sm" onClick={() => send("skip", { offsetMs: SKIP_FORWARD_MS })}>
              <RotateCw className="size-4" />
            </IconButton>
          ) : null}
          {nextShown ? (
            <IconButton
              label={live ? `Next channel on ${name}` : `Next on ${name}`}
              size="sm"
              disabled={!can("next")}
              onClick={() => send("next")}
            >
              <SkipForward className="size-4 fill-current" />
            </IconButton>
          ) : null}
        </div>
      ) : null}
      <span className="sr-only" role="status">
        {waiting ? `Waiting for ${name}` : ""}
      </span>

      <div className="flex items-center gap-1">
        {(["audio", "subtitles"] as const).map((p) =>
          offered[p] ? (
            <button
              key={p}
              ref={(el) => {
                pickers.current[p] = el;
              }}
              type="button"
              aria-label={pickerLabel(p)}
              aria-haspopup="menu"
              aria-expanded={picking === p}
              onClick={() => setPicking(picking === p ? null : p)}
              className={cn(
                "flex items-center gap-1.5 rounded-md px-2 py-1 text-xs font-medium transition-colors hover:bg-white/5 hover:text-fg",
                picking === p ? "text-fg" : "text-muted",
              )}
            >
              {p === "audio" ? <AudioLines className="size-3.5" /> : <Captions className="size-3.5" />}
              {p === "audio" ? "Audio" : "Subtitles"}
              <ChevronDown className={cn("size-3.5 transition-transform", picking === p && "rotate-180")} />
            </button>
          ) : null,
        )}
        <button
          type="button"
          onClick={() => {
            setError(null);
            stop.mutate(
              { sessionId: session.session_id },
              { onError: (err) => setError(`${sessionCommandError(err, name)}.`) },
            );
          }}
          disabled={stop.isPending}
          aria-label={`Stop on ${name}`}
          className="ml-auto flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-muted transition-colors hover:bg-danger/10 hover:text-danger disabled:opacity-50"
        >
          <CircleStop className="size-3.5" />
          Stop
        </button>
      </div>
      {picking && offered[picking] ? (
        <TrackMenu
          label={pickerLabel(picking)}
          options={options}
          current={picking === "audio" ? session.audio_index : session.subtitle_index}
          onPick={(index) => {
            closePicker();
            send(picking, { streamIndex: index });
          }}
          onClose={closePicker}
        />
      ) : null}

      {error ? (
        <p role="alert" className="text-xs text-danger">
          {error}
        </p>
      ) : null}
    </div>
  );
}

/**
 * The tracks a remote picks from, as the player's own menu names them, the
 * one playing ticked. It opens in the panel, below its button: a menu
 * floating over the panel would be cut off at its edge.
 */
function TrackMenu({
  label,
  options,
  current,
  onPick,
  onClose,
}: {
  label: string;
  options: { index: number; label: string }[];
  current: number | null;
  onPick: (index: number) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const items = () => Array.from(ref.current?.querySelectorAll<HTMLElement>('[role="menuitemradio"]') ?? []);
  // Opened, the track playing has focus (the first when none does).
  useEffect(() => {
    const list = Array.from(ref.current?.querySelectorAll<HTMLElement>('[role="menuitemradio"]') ?? []);
    (list.find((el) => el.getAttribute("aria-checked") === "true") ?? list[0])?.focus();
  }, []);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const list = items();
    const at = list.indexOf(document.activeElement as HTMLElement);
    let next: number;
    switch (e.key) {
      case "ArrowDown":
        next = at + 1 < list.length ? at + 1 : 0;
        break;
      case "ArrowUp":
        next = at > 0 ? at - 1 : list.length - 1;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = list.length - 1;
        break;
      case "Escape":
        // The menu closes, not the remote around it.
        e.stopPropagation();
        onClose();
        return;
      default:
        return;
    }
    e.preventDefault();
    list[next]?.focus();
  };

  return (
    <div ref={ref} role="menu" aria-label={label} onKeyDown={onKeyDown} className="max-h-60 overflow-y-auto rounded-md border border-border bg-surface py-1">
      {options.map((o) => (
        <button
          key={o.index}
          type="button"
          role="menuitemradio"
          aria-checked={o.index === current}
          onClick={() => onPick(o.index)}
          className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-sm text-fg outline-none transition-colors hover:bg-white/5 focus-visible:bg-white/10"
        >
          <Check className={cn("size-4 shrink-0 text-accent", o.index !== current && "invisible")} />
          <span className="min-w-0 flex-1 truncate">{o.label}</span>
        </button>
      ))}
    </div>
  );
}
