import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  AppWindow,
  AudioLines,
  Captions,
  Check,
  ChevronDown,
  ChevronUp,
  CircleStop,
  Laptop,
  MonitorSmartphone,
  Pause,
  Play,
  RotateCcw,
  RotateCw,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
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
import { formatTime } from "@/pages/livetv/common";
import { posterCandidates } from "@/lib/images";
import { receiverId, takesCasts } from "@/lib/castReceiver";
import { isPlayingHere, useSessionsHere } from "@/lib/playingHere";
import {
  ANSWER_WAIT_MS,
  asSent,
  commandAnswered,
  estimatedPosition,
  mayEndSession,
  newestOn,
  otherDevices,
  remoteClock,
  remoteOpened,
  SKIP_BACK_MS,
  SKIP_FORWARD_MS,
  trackLabel,
  useRemoteAsk,
  withEnded,
  type OtherDevice,
  type RemoteAsk,
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
 * itself). `ended`, a session a command ended (`mayEndSession`), is kept
 * while its device plays nothing else. `known`: both lists have been asked
 * for.
 */
export function useOtherDevices(ended: RemoteSession | null = null): { devices: OtherDevice[]; known: boolean } {
  const sessions = usePlayingSessions();
  const receivers = useRemoteDevices();
  const here = useSessionsHere();
  // The desktop app takes casts: its own receiver isn't another device either.
  const mine = takesCasts() ? receiverId() : null;
  const playing = (sessions.data ?? []).filter((s) => !here.has(s.session_id));
  const devices = otherDevices(
    (receivers.data ?? []).filter((r) => r.receiver_id !== mine),
    withEnded(playing, ended),
  );
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
 * the control around it to be described by. The device `chosen` has its
 * controls below, whose bar says how far: its row has none.
 */
export function DeviceSummary({ device, lineId, chosen }: { device: OtherDevice; lineId: string; chosen?: boolean }) {
  const nowMs = useNow(1_000) * 1000;
  const s = device.session;
  const progress = !chosen && s?.kind === "item" && s.duration_ms ? estimatedPosition(s, nowMs) / s.duration_ms : null;
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
 * plays there and the commands its player obeys. Only a device that plays
 * is chosen: one ready to play has nothing to control.
 */
export function RemoteButton() {
  // A next, or a track that needs a new session, ends one session to start
  // another: until the device plays the new one (or the wait for it is
  // over), the one that ended stays.
  const [ended, setEnded] = useState<RemoteSession | null>(null);
  const { devices, known } = useOtherDevices(ended);
  const after = ended && devices.find((d) => d.id === ended.device_id)?.session;
  if (ended && after?.session_id !== ended.session_id) setEnded(null);
  useEffect(() => {
    if (!ended) return;
    const timer = window.setTimeout(() => setEnded(null), ANSWER_WAIT_MS);
    return () => window.clearTimeout(timer);
  }, [ended]);
  const [open, setOpen] = useState(false);
  // Kept while the panel is open; with one device playing, that one is shown.
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

  // Play on…: the remote opens on the device it went to, its controls
  // showing once its session does.
  const ask = useRemoteAsk();
  const [heard, setHeard] = useState<RemoteAsk | null>(null);
  if (ask && ask !== heard) {
    setHeard(ask);
    setOpen(true);
    setChosen(ask.deviceId);
  }
  useEffect(() => {
    if (!ask) return;
    remoteOpened(ask);
    buttonRef.current?.focus();
  }, [ask]);
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

  const playing = devices.filter((d) => d.session);
  // A device chosen that plays nothing yet (Play on… just went to it) shows once it plays.
  const waitingFor = devices.some((d) => d.id === chosen && !d.session);
  const current = waitingFor
    ? undefined
    : (playing.find((d) => d.id === chosen) ?? (playing.length === 1 ? playing[0] : undefined));
  const label = playing.length
    ? `Remote, ${pluralize(playing.length, "device")} playing`
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
            {/* With a device's controls below, a long list scrolls on its own, about four and a half
                rows tall, so the controls stay in view; alone, it takes the panel. */}
            <ul className={cn(current && "max-h-[17rem] overflow-y-auto overscroll-contain")}>
              {devices.map((d) => (
                <li key={d.id}>
                  <DeviceRow device={d} chosen={d === current} onChoose={() => setChosen(d.id)} />
                </li>
              ))}
            </ul>
          </div>
          {current?.session ? (
            <div className="border-t border-border">
              <PlayingControls key={current.id} device={current} session={current.session} onEnding={setEnded} />
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

/** One device in the list: choosing one that plays shows its controls below. One ready to play is only listed. */
function DeviceRow({ device, chosen, onChoose }: { device: OtherDevice; chosen: boolean; onChoose: () => void }) {
  const lineId = useId();
  const ref = useRef<HTMLButtonElement>(null);
  // Chosen from elsewhere (Play on… opening the remote on it), it shows in a list scrolled away from it.
  useEffect(() => {
    if (chosen) ref.current?.scrollIntoView?.({ block: "nearest" });
  }, [chosen]);
  if (!device.session) {
    return (
      <div className="flex w-full items-center gap-3 px-2 py-2">
        <DeviceSummary device={device} lineId={lineId} />
      </div>
    );
  }
  return (
    <button
      ref={ref}
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

type Picker = "audio" | "subtitles" | "quality";

/** One entry of a picker's menu: its key (a stream index, or a quality's id) and its label. */
interface PickerOption {
  key: string;
  label: string;
}

const PICKER_NAMES: Record<Picker, string> = { audio: "Audio", subtitles: "Subtitles", quality: "Quality" };

function PickerIcon({ picker }: { picker: Picker }) {
  if (picker === "audio") return <AudioLines className="size-4" />;
  if (picker === "subtitles") return <Captions className="size-4" />;
  return <SlidersHorizontal className="size-4" />;
}

/**
 * A playing device's controls: what plays, how far, and the commands its
 * player listed. A command shows what it asks at once (`asSent`). One its
 * player may end the session for (`mayEndSession`) tells `onEnding`, for
 * the remote to keep that session until the new one plays.
 */
function PlayingControls({
  device,
  session,
  onEnding,
}: {
  device: OtherDevice;
  session: RemoteSession;
  onEnding: (ended: RemoteSession) => void;
}) {
  const name = device.name;
  const qc = useQueryClient();
  const command = useSessionCommand();
  const stop = useStopSession();
  const nowMs = useNow(1_000) * 1000;
  const [sent, setSent] = useState<SentCommand | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picking, setPicking] = useState<Picker | null>(null);
  const [confirming, setConfirming] = useState(false);
  const pickers = useRef<Partial<Record<Picker, HTMLButtonElement | null>>>({});
  const stopRef = useRef<HTMLButtonElement>(null);
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
          if (list && !commandAnswered(sent, newestOn(list, sent.before.device_id, isPlayingHere), remoteClock())) {
            setError(`${name} didn't answer.`);
          }
        });
    }, ANSWER_WAIT_MS);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [sent, qc, name]);

  // What the controls show: the session as its player reported it, with what was just asked of it.
  const shown = asSent(session, sent);
  const position = estimatedPosition(shown, nowMs);
  const duration = session.duration_ms;

  const send = (c: Control, extra: Pick<SessionCommand, "positionMs" | "offsetMs" | "streamIndex" | "quality"> = {}) => {
    setError(null);
    const sentAt = remoteClock();
    // A skip goes from where the bar shows, a seek still on its way too.
    const at = estimatedPosition(shown, sentAt);
    // A previous a few seconds in starts the title over, as players do.
    const target =
      extra.positionMs ??
      (extra.offsetMs != null
        ? Math.max(0, Math.min(at + extra.offsetMs, duration ?? Number.MAX_SAFE_INTEGER))
        : c === "previous" && session.kind !== "live"
          ? 0
          : undefined);
    // Shown at once, and waited for from now: the player's report confirms it.
    const entry: SentCommand = { command: c, before: session, sentAt, target, track: extra.streamIndex, quality: extra.quality };
    setSent(entry);
    if (mayEndSession(c)) onEnding(session);
    command.mutate(
      { sessionId: session.session_id, command: c, ...extra },
      {
        onError: (err) => {
          setSent((s) => (s === entry ? null : s));
          setError(`${sessionCommandError(err, name)}.`);
        },
      },
    );
  };

  const paused = shown.state === "paused";
  const toggle = paused ? (can("play") ? "play" : null) : can("pause") ? "pause" : null;
  const live = session.kind === "live";
  const kind = session.item?.kind;
  // Next is there for whatever has a next (an episode, a track, a queue's
  // entry, a channel), greyed out where the player doesn't take it.
  const nextShown = can("next") || live || kind === "episode" || kind === "track";
  // A channel goes back and on, and pauses where the player can: no skips.
  const skips = can("skip") && !live;
  const [title, detail] = titleLines(session);
  const waiting = command.isPending || !!sent;
  // What a next or previous plays isn't known until it plays: what plays now is dimmed until then.
  const changing = sent?.command === "next" || sent?.command === "previous";

  // Audio where there is a choice of tracks; subtitles where there are any,
  // with Off; quality where the player's menu offers some.
  const audio = session.tracks.audio;
  const subtitles = session.tracks.subtitles;
  const qualities = session.qualities ?? [];
  const offered: Record<Picker, boolean> = {
    audio: can("audio") && audio.length > 1,
    subtitles: can("subtitles") && subtitles.length > 0,
    quality: can("quality") && qualities.length > 0,
  };
  const options: Record<Picker, PickerOption[]> = {
    audio: audio.map((t) => ({ key: String(t.index), label: trackLabel(t, "audio") })),
    subtitles: [
      { key: "-1", label: "Off" },
      ...subtitles.map((t) => ({ key: String(t.index), label: trackLabel(t, "subtitle") })),
    ],
    quality: qualities.map((q) => ({ key: q.id, label: q.label })),
  };
  const currentOf = (p: Picker): string | null => {
    if (p === "quality") return shown.quality ?? null;
    const index = p === "audio" ? shown.audio_index : shown.subtitle_index;
    return index == null ? null : String(index);
  };
  const pick = (p: Picker, key: string) => {
    if (p === "quality") send("quality", { quality: key });
    else send(p, { streamIndex: Number(key) });
  };
  const closePicker = () => {
    if (picking) pickers.current[picking]?.focus();
    setPicking(null);
  };
  const pickerLabel = (p: Picker) => `${PICKER_NAMES[p]} on ${name}`;
  const pickersShown = (["audio", "subtitles", "quality"] as const).filter((p) => offered[p]);

  const askStop = () => {
    setPicking(null);
    setConfirming(true);
  };
  const cancelStop = () => {
    setConfirming(false);
    // Back on the button that asked, once it shows again.
    window.requestAnimationFrame(() => stopRef.current?.focus());
  };
  const confirmStop = () => {
    setError(null);
    setConfirming(false);
    stop.mutate(
      { sessionId: session.session_id },
      { onError: (err) => setError(`${sessionCommandError(err, name)}.`) },
    );
  };

  return (
    <div role="group" aria-label={name} aria-busy={waiting} className="space-y-3 px-4 py-3">
      <div className={cn("flex items-center gap-3 transition-opacity", changing && "opacity-50")}>
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

      {live ? (
        <LiveProgress channel={session.channel} nowMs={nowMs} />
      ) : duration ? (
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

      {live ? (
        // A TV remote's channel rocker: down and up, with pause beside it where the player can.
        <div className={cn("flex items-center justify-center gap-3 transition-opacity", changing && "opacity-50")}>
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
          <div className="flex items-center gap-0.5 rounded-full bg-white/5 p-1 ring-1 ring-inset ring-border">
            <IconButton label={`Previous channel on ${name}`} size="sm" disabled={!can("previous")} onClick={() => send("previous")}>
              <ChevronDown className="size-5" />
            </IconButton>
            <span aria-hidden className="px-1.5 text-[11px] font-semibold uppercase tracking-wider text-subtle">
              Channel
            </span>
            <IconButton label={`Next channel on ${name}`} size="sm" disabled={!can("next")} onClick={() => send("next")}>
              <ChevronUp className="size-5" />
            </IconButton>
          </div>
        </div>
      ) : can("previous") || skips || toggle || nextShown ? (
        <div className={cn("flex items-center justify-center gap-2 transition-opacity", changing && "opacity-50")}>
          {can("previous") ? (
            <IconButton label={`Previous on ${name}`} size="sm" onClick={() => send("previous")}>
              <SkipBack className="size-4 fill-current" />
            </IconButton>
          ) : null}
          {skips ? (
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
          {skips ? (
            <IconButton label={`Forward 30 seconds on ${name}`} size="sm" onClick={() => send("skip", { offsetMs: SKIP_FORWARD_MS })}>
              <RotateCw className="size-4" />
            </IconButton>
          ) : null}
          {nextShown ? (
            <IconButton label={`Next on ${name}`} size="sm" disabled={!can("next")} onClick={() => send("next")}>
              <SkipForward className="size-4 fill-current" />
            </IconButton>
          ) : null}
        </div>
      ) : null}
      <span className="sr-only" role="status">
        {waiting ? `Waiting for ${name}` : ""}
      </span>

      {confirming ? (
        <StopConfirm name={name} onCancel={cancelStop} onStop={confirmStop} />
      ) : (
        <div className="flex items-center gap-1">
          {pickersShown.map((p) => (
            <IconButton
              key={p}
              ref={(el) => {
                pickers.current[p] = el;
              }}
              label={pickerLabel(p)}
              size="sm"
              active={picking === p}
              aria-haspopup="menu"
              aria-expanded={picking === p}
              onClick={() => setPicking(picking === p ? null : p)}
              className={cn(picking === p && "bg-white/10")}
            >
              <PickerIcon picker={p} />
            </IconButton>
          ))}
          <button
            ref={stopRef}
            type="button"
            onClick={askStop}
            disabled={stop.isPending}
            aria-label={`Stop on ${name}`}
            aria-haspopup="dialog"
            className="ml-auto flex h-8 items-center gap-1.5 rounded-full px-3 text-xs font-medium text-muted transition-colors hover:bg-danger/10 hover:text-danger disabled:opacity-50"
          >
            <CircleStop className="size-4" />
            Stop
          </button>
        </div>
      )}
      {picking && offered[picking] && !confirming ? (
        <TrackMenu
          label={pickerLabel(picking)}
          options={options[picking]}
          current={currentOf(picking)}
          onPick={(key) => {
            const p = picking;
            closePicker();
            pick(p, key);
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
 * Stop asks first: what plays there ends for whoever watches it. Escape or
 * Cancel goes back; Stop has focus, as it was the button pressed.
 */
function StopConfirm({ name, onCancel, onStop }: { name: string; onCancel: () => void; onStop: () => void }) {
  const stopRef = useRef<HTMLButtonElement>(null);
  const titleId = useId();
  useEffect(() => stopRef.current?.focus(), []);
  return (
    <div
      role="alertdialog"
      aria-labelledby={titleId}
      onKeyDown={(e) => {
        if (e.key !== "Escape") return;
        // The question closes, not the remote around it.
        e.stopPropagation();
        onCancel();
      }}
      className="flex items-center gap-2 rounded-md bg-danger/10 py-1.5 pl-3 pr-1.5"
    >
      <span id={titleId} className="min-w-0 flex-1 truncate text-xs font-medium text-fg">
        Stop playing on {name}?
      </span>
      <button
        type="button"
        onClick={onCancel}
        className="h-7 rounded-full px-3 text-xs font-medium text-muted transition-colors hover:bg-white/10 hover:text-fg"
      >
        Cancel
      </button>
      <button
        ref={stopRef}
        type="button"
        onClick={onStop}
        className="h-7 rounded-full bg-danger px-3 text-xs font-semibold text-white transition-colors hover:bg-danger/90"
      >
        Stop
      </button>
    </div>
  );
}

/**
 * How far along the programme on a channel is, by the clock, from its start
 * to its end, and what's on next. Nothing without a guide.
 */
function LiveProgress({ channel, nowMs }: { channel: RemoteSession["channel"]; nowMs: number }) {
  const start = channel?.now_start;
  const stop = channel?.now_stop;
  const next = channel?.next;
  const timed = channel?.now && start != null && stop != null && stop > start;
  if (!timed && !next) return null;
  return (
    <div className="space-y-1">
      {timed ? (
        <div className="flex items-center gap-2 text-[11px] tabular-nums text-subtle">
          <span>{formatTime(start)}</span>
          <ProgressBar
            value={Math.min(1, Math.max(0, (nowMs / 1000 - start) / (stop - start)))}
            className="h-1 flex-1"
            label={`${channel.now}, ${formatTime(start)} to ${formatTime(stop)}`}
          />
          <span>{formatTime(stop)}</span>
        </div>
      ) : null}
      {next ? <div className="truncate text-xs text-subtle">Next: {next}</div> : null}
    </div>
  );
}

/**
 * The options a remote picks from (tracks as the player's own menu names
 * them, or qualities), the one playing ticked. It opens in the panel, below
 * its button: a menu floating over the panel would be cut off at its edge.
 */
function TrackMenu({
  label,
  options,
  current,
  onPick,
  onClose,
}: {
  label: string;
  options: PickerOption[];
  current: string | null;
  onPick: (key: string) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const items = () => Array.from(ref.current?.querySelectorAll<HTMLElement>('[role="menuitemradio"]') ?? []);
  // Opened, the option playing has focus (the first when none does).
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
          key={o.key}
          type="button"
          role="menuitemradio"
          aria-checked={o.key === current}
          onClick={() => onPick(o.key)}
          className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-sm text-fg outline-none transition-colors hover:bg-white/5 focus-visible:bg-white/10"
        >
          <Check className={cn("size-4 shrink-0 text-accent", o.key !== current && "invisible")} />
          <span className="min-w-0 flex-1 truncate">{o.label}</span>
        </button>
      ))}
    </div>
  );
}
