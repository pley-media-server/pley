import { useTranslation } from "react-i18next";
import { cn } from "@/lib/cn";
import { useConnectionLost } from "@/lib/connection";
import { appShell, SWITCH_SERVER_PATH } from "@/lib/shell";

/**
 * "Pley is offline" floating under the top bar while the server is out of
 * reach, over the page rather than pushing it down: the pages show what
 * they loaded before, and the app keeps asking the server until it answers.
 * In the desktop app it leads to the downloads, which play without the
 * server.
 */
export function ConnectionBanner() {
  const { t } = useTranslation("components");
  const lost = useConnectionLost();
  if (!lost) return null;
  const desktop = appShell() === "desktop";
  return (
    <div role="status" className="pointer-events-none sticky top-[4.75rem] z-10 flex h-0 justify-center px-3">
      <div
        className={cn(
          "pointer-events-auto flex h-10 animate-pop-in items-center gap-2.5 whitespace-nowrap rounded-full border border-border-strong bg-surface-3/95 pl-3.5 text-sm shadow-lg shadow-shade/25 backdrop-blur-md",
          desktop ? "pr-1.5" : "pr-4",
        )}
      >
        <span aria-hidden className="relative flex size-2.5 shrink-0">
          <span className="absolute inset-0 rounded-full bg-warning/70 motion-safe:animate-ping" />
          <span className="relative size-2.5 rounded-full bg-warning" />
        </span>
        <span className="font-medium text-fg">{t("connectionBanner.offline")}</span>
        <span className="hidden text-muted sm:inline">{t("connectionBanner.retrying")}</span>
        {desktop ? (
          <a
            href={SWITCH_SERVER_PATH}
            className="ml-1 rounded-full bg-fg/10 px-3 py-1 font-medium text-fg transition-colors hover:bg-fg/15"
          >
            {t("connectionBanner.downloads")}
          </a>
        ) : null}
      </div>
    </div>
  );
}
