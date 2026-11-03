import { Cast } from "lucide-react";
import { IconButton } from "@/components/ui";
import { useCast, type CastRequest } from "./context";

/**
 * Opens the "Play on" picker for what `request` gives. Shown only while the
 * profile has another device to cast to. `player`: in the video player's
 * controls, which are white on the picture.
 */
export function CastButton({ request, player }: { request: () => CastRequest; player?: boolean }) {
  const cast = useCast();
  if (!cast?.targets.length) return null;
  const label = "Play on another device";
  if (player) {
    return (
      <button
        type="button"
        aria-label={label}
        title={label}
        onClick={() => {
          // Full screen shows the player alone: the picker shows over the window.
          if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined);
          cast.choose(request());
        }}
        className="flex size-10 items-center justify-center rounded-full transition hover:bg-white/15"
      >
        <Cast className="size-5" />
      </button>
    );
  }
  return (
    <IconButton
      label={label}
      size="lg"
      aria-haspopup="dialog"
      className="bg-surface-3/80 hover:bg-border max-sm:size-11"
      onClick={() => cast.choose(request())}
    >
      <Cast className="size-5" />
    </IconButton>
  );
}
