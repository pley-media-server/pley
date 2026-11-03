import { useEffect, useState, type ReactNode } from "react";
import { useNavigate } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { LoaderCircle, Pause, Play, RotateCcw, RotateCw, Square, TriangleAlert } from "lucide-react";
import { castPosition, castQueries, type CastTarget } from "@/api/cast";
import { Artwork } from "@/components/Artwork";
import { Modal } from "@/components/Modal";
import { Slider } from "@/components/Slider";
import { Button } from "@/components/ui";
import { episodeLabel, formatClock } from "@/lib/format";
import { posterCandidates } from "@/lib/images";
import { playUrl } from "@/lib/usePlayItem";
import { useCast, type CastPending } from "./context";

/** How long a device sent a title has to say it plays it (docs/API.md). */
export const ANSWER_WITHIN_MS = 15_000;

/**
 * Whether a device sent a title has said it plays it: `null` once it has
 * (or nothing was sent), `waiting` until then, `unanswered` after
 * {@link ANSWER_WITHIN_MS}. What it said before the title was sent doesn't
 * count.
 */
export function castWait(pending: CastPending | null, device: CastTarget | undefined, now: number): "waiting" | "unanswered" | null {
  if (!pending) return null;
  const state = device?.state;
  if (state && state.item.id === pending.itemId && state.heard_at >= pending.sentAt) return null;
  return now - pending.sentAt >= ANSWER_WITHIN_MS ? "unanswered" : "waiting";
}

const STATE_LABELS = { playing: "Playing", paused: "Paused", buffering: "Loading" } as const;

/** A seek button with its seconds drawn in its arrow. */
function SkipIcon({ back, seconds }: { back: boolean; seconds: number }) {
  const Icon = back ? RotateCcw : RotateCw;
  return (
    <span className="relative flex size-7 items-center justify-center">
      <Icon className="size-7" strokeWidth={1.75} />
      <span className="absolute inset-0 flex items-center justify-center pt-0.5 text-[9px] font-bold tabular-nums">{seconds}</span>
    </span>
  );
}

function Notice({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return <div className="flex flex-col items-center gap-3 py-6 text-center text-sm text-muted">{icon}{children}</div>;
}

/**
 * A device's remote: what it plays, where, and its player's buttons. Until
 * the device says it plays what it was sent (`pending`), it waits for it.
 */
export function CastRemote({
  deviceId,
  name,
  pending,
  onAnswered,
  onClose,
}: {
  deviceId: string;
  /** The device's name when it was opened, should it leave the list. */
  name: string;
  pending: CastPending | null;
  onAnswered: () => void;
  onClose: () => void;
}) {
  const cast = useCast();
  const navigate = useNavigate();
  const device = useQuery(castQueries.devices()).data?.find((d) => d.id === deviceId);
  const deviceName = device?.name ?? name;
  const state = device?.state ?? null;

  // The clock moves a playing position on, and ends the wait.
  const [now, setNow] = useState(() => Date.now());
  const ticking = !!pending || state?.state === "playing";
  useEffect(() => {
    if (!ticking) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [ticking]);

  const wait = castWait(pending, device, Math.max(now, pending?.sentAt ?? 0));
  useEffect(() => {
    if (pending && wait === null) onAnswered();
  }, [pending, wait, onAnswered]);

  const steer = cast?.steer;
  let body: ReactNode;
  let footer: ReactNode = (
    <Button variant="ghost" onClick={onClose}>
      Close
    </Button>
  );
  if (wait === "waiting") {
    body = (
      <Notice icon={<LoaderCircle className="size-8 animate-spin" />}>
        <p>Waiting for {deviceName}…</p>
      </Notice>
    );
  } else if (wait === "unanswered") {
    body = (
      <Notice icon={<TriangleAlert className="size-8 text-accent" />}>
        <p className="font-medium text-fg">{deviceName} didn't answer</p>
        <p>Check that it is on, and that Pley is open on it.</p>
      </Notice>
    );
  } else if (!device) {
    body = (
      <Notice icon={<TriangleAlert className="size-8" />}>
        <p>{deviceName} isn't connected any more.</p>
      </Notice>
    );
  } else if (!state) {
    body = (
      <Notice icon={<Square className="size-8" />}>
        <p>Nothing is playing on {deviceName}.</p>
      </Notice>
    );
  } else {
    const item = state.item;
    const positionMs = castPosition(state, Math.max(now, state.heard_at));
    const durationMs = state.duration_ms ?? item.duration_ms ?? 0;
    const isEpisode = item.kind === "episode";
    const title = isEpisode ? (item.grandparent_title ?? item.title) : item.title;
    const subtitle = isEpisode ? [episodeLabel(item), item.title].filter(Boolean).join(" · ") : item.year ? String(item.year) : null;
    const playing = state.state !== "paused";
    const seek = (ms: number) => steer?.(deviceId, "seek", Math.max(0, durationMs > 0 ? Math.min(ms, durationMs) : ms));
    body = (
      <div className="space-y-4">
        <div className="flex gap-4">
          <Artwork
            candidates={posterCandidates(item)}
            width={96}
            alt=""
            title={title}
            className="aspect-[2/3] w-24 shrink-0 rounded-md"
          />
          <div className="min-w-0 flex-1 space-y-1 pt-1">
            <div className="line-clamp-2 text-base font-semibold">{title}</div>
            {subtitle ? <div className="truncate text-sm text-muted">{subtitle}</div> : null}
            <div className="text-xs text-subtle">
              {STATE_LABELS[state.state]} on {deviceName}
            </div>
          </div>
        </div>
        <div>
          <Slider label="Seek" value={positionMs} max={durationMs} onCommit={seek} />
          <div className="flex justify-between text-xs tabular-nums text-subtle">
            <span>{formatClock(positionMs / 1000)}</span>
            <span>{durationMs > 0 ? formatClock(durationMs / 1000) : ""}</span>
          </div>
        </div>
        <div className="flex items-center justify-center gap-4">
          <button
            type="button"
            aria-label="Back 10 seconds"
            title="Back 10 seconds"
            onClick={() => seek(positionMs - 10_000)}
            className="flex size-11 items-center justify-center rounded-full text-fg transition hover:bg-white/10"
          >
            <SkipIcon back seconds={10} />
          </button>
          <button
            type="button"
            aria-label={playing ? "Pause" : "Play"}
            onClick={() => steer?.(deviceId, playing ? "pause" : "resume")}
            className="flex size-14 items-center justify-center rounded-full bg-fg text-bg transition hover:scale-105"
          >
            {playing ? <Pause className="size-6 fill-current" /> : <Play className="ml-0.5 size-6 fill-current" />}
          </button>
          <button
            type="button"
            aria-label="Forward 30 seconds"
            title="Forward 30 seconds"
            onClick={() => seek(positionMs + 30_000)}
            className="flex size-11 items-center justify-center rounded-full text-fg transition hover:bg-white/10"
          >
            <SkipIcon back={false} seconds={30} />
          </button>
        </div>
      </div>
    );
    footer = (
      <>
        <Button
          variant="secondary"
          icon={<Square className="size-4 fill-current" />}
          onClick={() => {
            steer?.(deviceId, "stop");
            onClose();
          }}
        >
          Stop
        </Button>
        <Button
          variant="primary"
          icon={<Play className="size-4 fill-current" />}
          onClick={() => {
            // The device stops; this one plays on from where it was.
            steer?.(deviceId, "stop");
            onClose();
            void navigate(playUrl(item.id, { startMs: positionMs }));
          }}
        >
          Play here
        </Button>
      </>
    );
  }

  return (
    <Modal open onClose={onClose} title={deviceName || "Play on"} size="sm" footer={footer}>
      {body}
    </Modal>
  );
}
