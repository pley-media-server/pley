import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check } from "lucide-react";
import { useItem } from "@/api/hooks";
import type { Chapter } from "@/api/types";
import { Menu, MenuItem } from "@/components/Menu";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { ChaptersMenu } from "@/player/ChaptersMenu";
import { BOOK_RATES, useAudioPlayer, useAudioTime } from "./AudioPlayer";

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
            "h-8 min-w-12 rounded-full px-2 text-xs font-semibold tabular-nums transition hover:bg-white/10",
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

export function BookChapters({ chapters }: { chapters: Chapter[] }) {
  const player = useAudioPlayer();
  const time = useAudioTime();
  const [open, setOpen] = useState(false);
  if (chapters.length < 2) return null;
  return <ChaptersMenu chapters={chapters} current={time.position} open={open} setOpen={setOpen} onSeek={player.seek} />;
}
