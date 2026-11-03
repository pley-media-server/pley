import { Cast, Pause, Play } from "lucide-react";
import { IconButton } from "@/components/ui";
import { cn } from "@/lib/cn";
import { fullTitle } from "@/lib/format";
import { useCast } from "./context";

/** The device last cast to, while it plays something: what plays there, as a bar at the bottom of the pages. */
export function useCastBar() {
  const cast = useCast();
  const device = cast?.last ? cast.targets.find((d) => d.id === cast.last) : undefined;
  return device?.state ? { device, state: device.state } : null;
}

/**
 * "Playing on" the device last cast to: its title and play/pause; the rest
 * opens its remote. `raised`: over the music player's bar.
 */
export function CastBar({ raised }: { raised?: boolean }) {
  const cast = useCast();
  const shown = useCastBar();
  if (!cast || !shown) return null;
  const { device, state } = shown;
  const playing = state.state !== "paused";
  return (
    <div
      className={cn(
        "fixed inset-x-0 z-40 border-t border-border bg-surface/95 backdrop-blur-md lg:left-60",
        raised ? "bottom-[78px] md:bottom-[77px]" : "bottom-0",
      )}
    >
      <div className="flex h-14 items-center gap-3 px-3 sm:px-5">
        <button
          type="button"
          onClick={() => cast.showRemote(device.id)}
          className="flex min-w-0 flex-1 items-center gap-3 text-left"
        >
          <Cast className="size-5 shrink-0 text-accent" aria-hidden />
          <span className="min-w-0">
            <span className="block truncate text-xs text-muted">Playing on {device.name}</span>
            <span className="block truncate text-sm font-semibold">{fullTitle(state.item)}</span>
          </span>
        </button>
        <IconButton label={playing ? "Pause" : "Play"} size="md" onClick={() => cast.steer(device.id, playing ? "pause" : "resume")}>
          {playing ? <Pause className="size-5 fill-current" /> : <Play className="ml-0.5 size-5 fill-current" />}
        </IconButton>
      </div>
    </div>
  );
}
