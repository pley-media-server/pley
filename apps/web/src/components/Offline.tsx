import { RotateCw, Unplug } from "lucide-react";
import { useTranslation } from "react-i18next";
import { isApiError, errorMessage } from "@/api/client";
import { AuthBackground } from "@/components/AuthBackground";
import { LogoMark } from "@/components/Logo";
import { Button } from "@/components/ui";
import { cn } from "@/lib/cn";
import { useSetupFrame } from "@/lib/setupFrame";
import { useOutsideProfile } from "@/theme";

/** Pley's mark, unplugged, breathing softly while the server can't be reached; in the theme's colors. */
export function OfflineMark({ className }: { className?: string }) {
  return (
    <span aria-hidden className={cn("relative inline-flex", className)}>
      <span className="absolute -inset-2 rounded-[30%] bg-accent/20 motion-safe:animate-breathe" />
      <LogoMark className="relative size-full" />
      <span className="absolute -bottom-1.5 -right-1.5 flex size-[44%] items-center justify-center rounded-full bg-surface-3 text-warning shadow-md ring-4 ring-bg">
        <Unplug className="size-[58%]" strokeWidth={2.25} />
      </span>
    </span>
  );
}

/**
 * The server doesn't answer as the app starts: what to check, and a way to
 * try again. A network error says where to look; another says what came back.
 */
export function OfflineScreen({ error, onRetry }: { error: unknown; onRetry: () => void }) {
  const { t } = useTranslation(["pages", "common"]);
  // Before the server answers, no profile is known: Pley's own Midnight.
  useOutsideProfile();
  const inSetup = useSetupFrame() !== null;
  return (
    <div className={cn("relative flex items-center justify-center overflow-hidden px-6", inSetup ? "py-6" : "min-h-screen")}>
      {inSetup ? null : <AuthBackground />}
      <div role="alert" className="relative flex max-w-sm animate-pop-in flex-col items-center text-center">
        <OfflineMark className="mb-8 size-16" />
        <h1 className="text-xl font-semibold tracking-tight text-fg">{t("auth.cantReachServer")}</h1>
        <p className="mt-2 text-sm leading-relaxed text-balance text-muted">{isApiError(error, 0) ? t("auth.cantReachHint") : errorMessage(error)}</p>
        <Button variant="primary" className="mt-7" onClick={onRetry}>
          <RotateCw className="size-4" aria-hidden />
          {t("common:actions.tryAgain")}
        </Button>
      </div>
    </div>
  );
}
