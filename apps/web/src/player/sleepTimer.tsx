import { useEffect, useEffectEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Moon } from "lucide-react";
import { cn } from "@/lib/cn";

/** What the sleep timer can be set to: minutes, or the end of what plays. */
export type SleepChoice = 15 | 30 | 45 | 60 | 90 | "end";

export const SLEEP_MINUTES = [15, 30, 45, 60, 90] as const;

/** A running timer: it pauses at `endsAt` (Unix ms), or at the end of what plays. */
export type SleepTimer = { minutes: number; endsAt: number } | { end: true };

/** The notice shows this long before the timer pauses. */
export const SLEEP_NOTICE_S = 30;

export function sleepChoice(timer: SleepTimer | null): SleepChoice | null {
  if (!timer) return null;
  return "end" in timer ? "end" : (timer.minutes as SleepChoice);
}

/** A timer handed over to the next episode (in the router's state), if it is still running. */
export function carriedSleepTimer(state: unknown, now = Date.now()): SleepTimer | null {
  const timer = (state as { sleep?: unknown } | null)?.sleep as Partial<{ minutes: number; endsAt: number }> | undefined;
  return timer && typeof timer.minutes === "number" && typeof timer.endsAt === "number" && timer.endsAt > now
    ? { minutes: timer.minutes, endsAt: timer.endsAt }
    : null;
}

/**
 * The player's sleep timer: off at first (it isn't remembered), pausing
 * once its time is up (`onSleep`). `mediaLeft`: the seconds left of what
 * plays, for "end of this episode".
 */
export function useSleepTimer({ initial, mediaLeft, onSleep }: { initial: SleepTimer | null; mediaLeft: number; onSleep: () => void }) {
  const [timer, setTimer] = useState<SleepTimer | null>(initial);
  const [now, setNow] = useState(() => Date.now());
  const sleep = useEffectEvent(() => onSleep());

  const endsAt = timer && "endsAt" in timer ? timer.endsAt : null;
  useEffect(() => {
    if (endsAt === null) return;
    const tick = () => {
      const t = Date.now();
      setNow(t);
      if (t >= endsAt) {
        setTimer(null);
        sleep();
      }
    };
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, [endsAt]);

  const secondsLeft = !timer ? null : "endsAt" in timer ? Math.max(0, Math.ceil((timer.endsAt - now) / 1000)) : Math.max(0, Math.ceil(mediaLeft));

  return {
    timer,
    /** Whole seconds until it pauses (`null`: off). */
    secondsLeft,
    /** Seconds left while the notice shows. */
    notice: secondsLeft !== null && secondsLeft <= SLEEP_NOTICE_S ? secondsLeft : null,
    atEnd: !!timer && "end" in timer,
    choose: (choice: SleepChoice | null) => {
      const t = Date.now();
      setNow(t);
      setTimer(choice === null ? null : choice === "end" ? { end: true } : { minutes: choice, endsAt: t + choice * 60_000 });
    },
    /** The same time again (from when it would have paused); "end of this episode" just stops. */
    keepWatching: () => setTimer((old) => (old && "endsAt" in old ? { ...old, endsAt: old.endsAt + old.minutes * 60_000 } : null)),
    clear: () => setTimer(null),
  };
}

/** "Pausing in 30 s", with a way to keep watching. */
export function SleepNotice({ seconds, onKeepWatching, lifted }: { seconds: number; onKeepWatching: () => void; lifted: boolean }) {
  const { t } = useTranslation("player");
  return (
    <div
      role="status"
      className={cn(
        "absolute left-4 z-20 flex animate-pop-in items-center gap-3 rounded-md border border-white/25 bg-black/80 py-1.5 pl-4 pr-1.5 text-sm text-white shadow-2xl transition-[bottom] duration-300 sm:left-8",
        lifted ? "bottom-28" : "bottom-10",
      )}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
    >
      <Moon className="size-4 shrink-0 text-white/70" />
      <span className="tabular-nums">{t("sleep.pausingIn", { seconds })}</span>
      <button
        type="button"
        onClick={onKeepWatching}
        className="h-8 rounded px-3 text-sm font-semibold transition-colors hover:bg-white/15 focus-visible:bg-white/15"
      >
        {t("sleep.keepWatching")}
      </button>
    </div>
  );
}
