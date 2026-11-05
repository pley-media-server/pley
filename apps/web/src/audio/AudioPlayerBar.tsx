import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import {
  ListMusic,
  LoaderCircle,
  Pause,
  Play,
  Shuffle,
  SkipBack,
  SkipForward,
  Volume1,
  Volume2,
  VolumeX,
  X,
} from "lucide-react";
import { Artwork } from "@/components/Artwork";
import { Slider } from "@/components/Slider";
import { IconButton } from "@/components/ui";
import { cn } from "@/lib/cn";
import { formatClock, formatDuration } from "@/lib/format";
import { useDismiss } from "@/lib/hooks";
import { posterCandidates } from "@/lib/images";
import { useAudioPlayer, useAudioTime } from "./AudioPlayer";
import { itemUrl } from "@/lib/urls";

function QueuePopover() {
  const { t } = useTranslation("player");
  const player = useAudioPlayer();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useDismiss(ref, open, () => setOpen(false));
  return (
    <div ref={ref} className="relative">
      <IconButton label={t("music.queue")} size="sm" active={open} aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <ListMusic className="size-4" />
      </IconButton>
      {open ? (
        <div className="absolute bottom-full right-0 mb-3 max-h-96 w-80 animate-pop-in overflow-y-auto rounded-lg border border-border bg-surface-2 py-2 shadow-2xl shadow-black/60">
          <div className="px-4 pb-2 text-xs font-medium text-subtle">{t("upNext.title")}</div>
          {player.queue.map((t, i) => (
            <button
              key={`${t.id}-${i}`}
              type="button"
              aria-current={i === player.index ? "true" : undefined}
              onClick={() => player.jumpTo(i)}
              className={cn(
                "flex w-full items-center gap-3 px-4 py-1.5 text-left text-sm transition-colors hover:bg-white/5",
                i === player.index && "text-accent",
              )}
            >
              <span className="w-5 shrink-0 text-right text-xs tabular-nums text-subtle">{i + 1}</span>
              <span className="min-w-0 flex-1">
                <span className="block truncate">{t.title}</span>
                <span className="block truncate text-xs text-subtle">{t.grandparent_title ?? t.parent_title}</span>
              </span>
              <span className="shrink-0 text-xs tabular-nums text-subtle">{formatDuration(t.duration_ms)}</span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function AudioPlayerBar() {
  const { t } = useTranslation(["player", "common"]);
  const player = useAudioPlayer();
  const time = useAudioTime();
  const track = player.current;
  if (!track) return null;

  const VolumeIcon = player.muted || player.volume === 0 ? VolumeX : player.volume < 0.5 ? Volume1 : Volume2;
  const albumHref = track.parent_id ? itemUrl(track.parent_id) : itemUrl(track.id);

  return (
    <div className="fixed inset-x-0 bottom-0 z-40 border-t border-border bg-surface/95 backdrop-blur-md lg:left-60">
      {/* Mobile progress line */}
      <div className="h-0.5 bg-white/10 md:hidden">
        <div className="h-full bg-accent" style={{ width: `${time.duration ? (time.position / time.duration) * 100 : 0}%` }} />
      </div>
      <div className="flex h-[76px] items-center gap-3 px-3 sm:px-5">
        <Link to={albumHref} className="flex min-w-0 flex-1 items-center gap-3 md:w-64 md:flex-none lg:w-72">
          <Artwork
            candidates={posterCandidates(track)}
            width={56}
            alt=""
            title={track.parent_title ?? track.title}
            className="size-12 shrink-0 rounded"
          />
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold">{track.title}</div>
            <div className="truncate text-xs text-muted">
              {[track.grandparent_title, track.parent_title].filter(Boolean).join(" · ")}
            </div>
            {player.error ? <div className="truncate text-xs text-danger">{player.error}</div> : null}
          </div>
        </Link>

        <div className="flex flex-1 flex-col items-center justify-center gap-0.5 md:max-w-2xl">
          <div className="flex items-center gap-1 sm:gap-2">
            <IconButton
              label={player.shuffle ? t("music.shuffleOn") : t("music.shuffleOff")}
              size="sm"
              active={player.shuffle}
              onClick={player.toggleShuffle}
              className="hidden sm:inline-flex"
            >
              <Shuffle className="size-4" />
            </IconButton>
            <IconButton label={t("common:actions.previous")} size="sm" onClick={player.prev} className="hidden sm:inline-flex">
              <SkipBack className="size-4 fill-current" />
            </IconButton>
            <button
              type="button"
              aria-label={player.playing ? t("controls.pause") : t("common:actions.play")}
              onClick={player.toggle}
              className="flex size-10 items-center justify-center rounded-full bg-fg text-bg transition hover:scale-105"
            >
              {player.loading ? (
                <LoaderCircle className="size-5 animate-spin" />
              ) : player.playing ? (
                <Pause className="size-5 fill-current" />
              ) : (
                <Play className="ml-0.5 size-5 fill-current" />
              )}
            </button>
            <IconButton
              label={t("common:actions.next")}
              size="sm"
              onClick={player.next}
              disabled={player.index >= player.queue.length - 1}
            >
              <SkipForward className="size-4 fill-current" />
            </IconButton>
          </div>
          <div className="hidden w-full items-center gap-2.5 md:flex">
            <span className="w-11 text-right text-[11px] tabular-nums text-subtle">{formatClock(time.position)}</span>
            <Slider
              label={t("controls.seek")}
              value={time.position}
              max={time.duration}
              buffered={time.buffered}
              onCommit={player.seek}
              className="flex-1"
            />
            <span className="w-11 text-[11px] tabular-nums text-subtle">{formatClock(time.duration)}</span>
          </div>
        </div>

        <div className="hidden items-center justify-end gap-1 md:flex md:w-64 lg:w-72">
          <QueuePopover />
          <IconButton label={player.muted ? t("controls.unmute") : t("controls.mute")} size="sm" onClick={player.toggleMute}>
            <VolumeIcon className="size-4" />
          </IconButton>
          <Slider
            label={t("controls.volume")}
            value={player.muted ? 0 : player.volume}
            max={1}
            onPreview={player.setVolume}
            onCommit={player.setVolume}
            className="w-24"
          />
          <IconButton label={t("music.close")} size="sm" onClick={player.close}>
            <X className="size-4" />
          </IconButton>
        </div>
        <IconButton label={t("music.close")} size="sm" onClick={player.close} className="md:hidden">
          <X className="size-4" />
        </IconButton>
      </div>
    </div>
  );
}
