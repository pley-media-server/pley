import { useTranslation } from "react-i18next";
import type { WatchingHours } from "@/api/types";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";

/** Monday first; the server counts the week from Sunday (0). */
const DAYS = [1, 2, 3, 4, 5, 6, 0];
/** 7 January 2024 was a Sunday: day `d` of the server's week is 7 + d. */
const dayName = (d: number) => new Intl.DateTimeFormat(currentLocale(), { weekday: "short" }).format(new Date(2024, 0, 7 + d));
const hourName = (h: number) => new Intl.DateTimeFormat(currentLocale(), { hour: "numeric" }).format(new Date(2024, 0, 7, h));
export const hourOfDay = (h: number) =>
  new Intl.DateTimeFormat(currentLocale(), { hour: "numeric", minute: "2-digit" }).format(new Date(2024, 0, 7, h));

/** How much a cell shows of the accent: from a trace of it (rarely watched) to all of it. */
const shade = (playing: number) => `color-mix(in oklab, var(--color-accent) ${Math.round(8 + 92 * Math.min(1, playing))}%, var(--color-surface-2))`;

/**
 * The week as the server learned it: how much of each hour something
 * played, and the hours work on the rest of the libraries may begin in.
 */
export function WatchingHoursChart({ week }: { week: WatchingHours }) {
  const { t } = useTranslation("settings");
  return (
    <div className="space-y-2">
      {/* The cells shrink with the page: a phone shows the whole week too. */}
      <div className="grid grid-cols-[2rem_repeat(24,minmax(0,1fr))] gap-px text-[10px] text-subtle sm:gap-[2px]">
        <span />
        {[0, 6, 12, 18].map((h) => (
          <span key={h} className="col-span-6 truncate">
            {hourName(h)}
          </span>
        ))}
        {DAYS.map((d) => (
          <Day key={d} day={d} week={week} />
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-subtle">
        <span className="flex items-center gap-1.5">
          {t("watchingHours.rarely")}
          {[0, 0.33, 0.66, 1].map((p) => (
            <span key={p} className="size-3 rounded-sm" style={{ background: shade(p) }} />
          ))}
          {t("watchingHours.often")}
        </span>
        <span className="flex items-center gap-1.5">
          <span className="size-3 rounded-sm border border-dashed border-border-strong" /> {t("watchingHours.notSeenYet")}
        </span>
        <span className="flex items-center gap-1.5">
          <span className="flex size-3 items-center justify-center rounded-sm bg-surface-3">
            <span className="size-1 rounded-full bg-fg" />
          </span>
          {t("watchingHours.mayBegin")}
        </span>
      </div>
    </div>
  );
}

function Day({ day, week }: { day: number; week: WatchingHours }) {
  const { t } = useTranslation("settings");
  const name = dayName(day);
  return (
    <>
      <span className="self-center">{name}</span>
      {Array.from({ length: 24 }, (_, h) => {
        const at = day * 24 + h;
        const { playing, open } = week.hours[at] ?? { playing: null, open: false };
        const what = playing == null ? t("watchingHours.notSeenEnough") : t("watchingHours.played", { percent: Math.round(playing * 100) });
        const label = t(open ? "watchingHours.hourOpen" : "watchingHours.hour", { day: name, hour: hourOfDay(h), what });
        return (
          <span
            key={h}
            title={label}
            aria-label={label}
            role="img"
            tabIndex={0}
            className={cn(
              "flex aspect-square items-center justify-center rounded-[3px] outline-none transition hover:brightness-125 focus-visible:ring-2 focus-visible:ring-accent",
              playing == null && "border border-dashed border-border-strong",
              at === week.now && "ring-2 ring-fg",
            )}
            style={playing == null ? undefined : { background: shade(playing) }}
          >
            {open ? <span className="size-1 rounded-full bg-fg/90" /> : null}
          </span>
        );
      })}
    </>
  );
}
