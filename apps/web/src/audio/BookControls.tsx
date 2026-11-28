import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Moon } from "lucide-react";
import { useItem } from "@/api/hooks";
import type { Chapter } from "@/api/types";
import { Menu, MenuItem, MenuSeparator } from "@/components/Menu";
import { IconButton } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { formatClock } from "@/lib/format";
import { ChaptersMenu, chapterIndexAt } from "@/player/ChaptersMenu";
import { BOOK_RATES, useAudioPlayer, useAudioTime } from "./AudioPlayer";

/** Minutes the sleep timer offers. */
const SLEEP_MINUTES = [15, 30, 45, 60];

/** "1.25×", in the language shown. */
function rateLabel(rate: number): string {
  return `${new Intl.NumberFormat(currentLocale(), { maximumFractionDigits: 2 }).format(rate)}×`;
}

/** The chapters of the part playing (an .m4b's), none for a part without. */
export function usePartChapters(): Chapter[] {
  const player = useAudioPlayer();
  const detail = useItem(player.book ? player.current?.id : null);
  return detail.data?.files[0]?.chapters ?? [];
}

export function SpeedMenu() {
  const { t } = useTranslation("player");
  const player = useAudioPlayer();
  return (
    <Menu
      placement="above"
      trigger={({ open, toggle }) => (
        <button
          type="button"
          aria-label={t("book.speed", { rate: rateLabel(player.rate) })}
          aria-haspopup="menu"
          aria-expanded={open}
          onClick={toggle}
          className={cn(
            "h-8 min-w-12 rounded-full px-2 text-xs font-semibold tabular-nums transition hover:bg-fg/10",
            (open || player.rate !== 1) && "text-accent",
          )}
        >
          {rateLabel(player.rate)}
        </button>
      )}
    >
      {(close) =>
        BOOK_RATES.map((rate) => (
          <MenuItem
            key={rate}
            icon={rate === player.rate ? <Check /> : <span />}
            onClick={() => (close(), player.setRate(rate))}
          >
            {rateLabel(rate)}
          </MenuItem>
        ))
      }
    </Menu>
  );
}

/** Minutes and seconds until the timer's time, ticking. */
function useTimeLeft(until: number | null): string | null {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (until == null) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [until]);
  return until == null ? null : formatClock(Math.max(0, (until - now) / 1000));
}

export function SleepMenu({ chapters }: { chapters: Chapter[] }) {
  const { t } = useTranslation("player");
  const player = useAudioPlayer();
  const time = useAudioTime();
  const current = player.current;
  const sleep = player.sleep;
  const left = useTimeLeft(sleep && "until" in sleep ? sleep.until : null);
  if (!current) return null;

  const chapter = chapterIndexAt(chapters, time.position * 1000 + 250);
  const chapterEnd = chapter >= 0 ? chapters[chapter]?.end_ms : undefined;
  const partEnd = (current.duration_ms ?? 0) || Math.round(time.duration * 1000);
  const label = sleep
    ? "until" in sleep
      ? t("book.sleepIn", { time: left })
      : t("book.sleepAtEnd")
    : t("book.sleep");

  return (
    <Menu
      placement="above"
      trigger={({ open, toggle }) => (
        <span className="flex items-center">
          <IconButton label={label} size="sm" active={open || !!sleep} aria-haspopup="menu" aria-expanded={open} onClick={toggle}>
            <Moon className="size-4" />
          </IconButton>
          {left ? <span className="w-10 text-[11px] tabular-nums text-accent">{left}</span> : null}
        </span>
      )}
    >
      {(close) => (
        <>
          {SLEEP_MINUTES.map((m) => (
            <MenuItem key={m} onClick={() => (close(), player.setSleep({ until: Date.now() + m * 60_000 }))}>
              {t("sleep.minutes", { count: m })}
            </MenuItem>
          ))}
          {chapterEnd ? (
            <MenuItem onClick={() => (close(), player.setSleep({ itemId: current.id, atMs: chapterEnd }))}>
              {t("book.sleepEndOfChapter")}
            </MenuItem>
          ) : partEnd ? (
            <MenuItem onClick={() => (close(), player.setSleep({ itemId: current.id, atMs: partEnd }))}>
              {t("book.sleepEndOfPart")}
            </MenuItem>
          ) : null}
          {sleep ? (
            <>
              <MenuSeparator />
              <MenuItem onClick={() => (close(), player.setSleep(null))}>
                {t("book.sleepOff")}
              </MenuItem>
            </>
          ) : null}
        </>
      )}
    </Menu>
  );
}

export function BookChapters({ chapters }: { chapters: Chapter[] }) {
  const player = useAudioPlayer();
  const time = useAudioTime();
  const [open, setOpen] = useState(false);
  if (chapters.length < 2) return null;
  return <ChaptersMenu chapters={chapters} current={time.position} open={open} setOpen={setOpen} onSeek={player.seek} />;
}
