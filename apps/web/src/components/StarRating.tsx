import { useState, type KeyboardEvent } from "react";
import { Star } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useSetRating } from "@/api/hooks";
import type { ItemSummary } from "@/api/types";
import { useToast } from "@/components/Toast";
import { cn } from "@/lib/cn";

/**
 * The user's own rating: five stars, halves allowed (0-10 on the server).
 * Clicking the current rating clears it.
 */
export function StarRating({ item, className }: { item: ItemSummary; className?: string }) {
  const { t } = useTranslation("components");
  const toast = useToast();
  const setRating = useSetRating();
  const saved = item.user_state.rating ?? null;
  // The value being saved shows right away.
  const value = setRating.isPending && setRating.variables ? setRating.variables.rating : saved;
  const [hover, setHover] = useState<number | null>(null);
  const shown = hover ?? value ?? 0;
  /** 0-10 → "3.5 stars". */
  const label = (rating: number) => t("starRating.stars", { count: rating / 2 });

  const save = (rating: number | null) =>
    setRating.mutate({ id: item.id, rating }, { onError: (err) => toast.error(errorMessage(err)) });

  /** The rating under the pointer: the left half of a star is a half star. */
  const pointValue = (e: { clientX: number; currentTarget: HTMLElement }, star: number) => {
    const rect = e.currentTarget.getBoundingClientRect();
    return star * 2 - (e.clientX - rect.left < rect.width / 2 ? 1 : 0);
  };

  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const current = value ?? 0;
    const next =
      e.key === "ArrowRight" || e.key === "ArrowUp"
        ? Math.min(10, current + 1)
        : e.key === "ArrowLeft" || e.key === "ArrowDown"
          ? Math.max(0, current - 1)
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? 10
              : undefined;
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      save(null);
    } else if (next !== undefined) {
      e.preventDefault();
      save(next === 0 ? null : next);
    }
  };

  return (
    <div
      role="slider"
      tabIndex={0}
      aria-label={t("starRating.label")}
      aria-valuemin={0}
      aria-valuemax={10}
      aria-valuenow={value ?? 0}
      aria-valuetext={value ? label(value) : t("starRating.notRated")}
      title={value ? t("starRating.yourRating", { stars: label(value) }) : t("starRating.rateIt")}
      onKeyDown={onKey}
      onPointerLeave={() => setHover(null)}
      className={cn("inline-flex items-center rounded-md outline-none focus-visible:ring-2 focus-visible:ring-accent", className)}
    >
      {[1, 2, 3, 4, 5].map((star) => {
        const fill = Math.max(0, Math.min(1, shown / 2 - (star - 1)));
        return (
          <button
            key={star}
            type="button"
            tabIndex={-1}
            aria-hidden
            onPointerMove={(e) => setHover(pointValue(e, star))}
            onClick={(e) => {
              const picked = pointValue(e, star);
              save(picked === value ? null : picked);
            }}
            className="relative p-0.5"
          >
            <Star className={cn("size-[18px] transition-colors", hover !== null ? "text-accent/60" : "text-fg/35")} />
            {fill > 0 ? (
              <span className="absolute inset-0 overflow-hidden p-0.5" style={{ width: fill === 1 ? undefined : "50%" }}>
                <Star className="size-[18px] fill-accent text-accent" />
              </span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
