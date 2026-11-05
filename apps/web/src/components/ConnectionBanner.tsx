import { ServerOff } from "lucide-react";
import { useConnectionLost } from "@/lib/connection";
import { appShell, SWITCH_SERVER_PATH } from "@/lib/shell";

/**
 * "Pley is offline" under the top bar while the server is out of reach: the
 * pages show what they loaded before, and the app keeps asking the server
 * until it answers. In the desktop app it leads to the downloads, which play
 * without the server.
 */
export function ConnectionBanner() {
  const lost = useConnectionLost();
  if (!lost) return null;
  return (
    <div
      role="status"
      className="sticky top-16 z-10 flex items-center gap-3 border-b border-border bg-surface-2/95 px-3 py-2.5 text-sm text-fg/90 backdrop-blur-md sm:px-6 lg:px-8"
    >
      <ServerOff className="size-4 shrink-0 text-muted" aria-hidden />
      <p className="min-w-0 flex-1">Pley is offline</p>
      {appShell() === "desktop" ? (
        <a href={SWITCH_SERVER_PATH} className="shrink-0 font-medium text-accent hover:underline">
          Downloads
        </a>
      ) : null}
    </div>
  );
}
