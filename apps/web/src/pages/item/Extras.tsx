import { useCallback, useLayoutEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ItemSummary } from "@/api/types";
import { MediaCard } from "@/components/cards";
import { ArrowButton, useFittedRow } from "@/components/HubRow";
import { SectionTitle } from "@/components/ui";
import { t as translate } from "@/i18n";
import { smoothScroll } from "@/lib/hooks";
import { formatDuration } from "@/lib/format";
import { playUrl } from "@/lib/usePlayItem";

/** The kinds of extra with a name of their own; any other is an "Extra". */
const EXTRA_TYPES = ["trailer", "behind_the_scenes", "deleted_scene", "featurette", "interview", "scene", "short", "other"] as const;

function isExtraType(type: string): type is (typeof EXTRA_TYPES)[number] {
  return (EXTRA_TYPES as readonly string[]).includes(type);
}

/** "Trailer · 2m" */
export function extraSubtitle(item: Pick<ItemSummary, "extra_type" | "duration_ms">): string {
  const type = item.extra_type ?? "other";
  const label = translate(`extras.types.${isExtraType(type) ? type : "other"}`, { ns: "item" });
  return [label, formatDuration(item.duration_ms)].filter(Boolean).join(" · ");
}

/** Smallest card width: [mobile, desktop]. */
const CARD_WIDTHS: [number, number] = [200, 260];

/** Trailers and other bonus material; a card plays its extra. */
export function ExtrasRow({ extras }: { extras?: ItemSummary[] }) {
  const { t } = useTranslation("item");
  const { ref, width, perPage, gap } = useFittedRow(CARD_WIDTHS);
  const [canLeft, setCanLeft] = useState(false);
  const [canRight, setCanRight] = useState(false);
  const count = extras?.length ?? 0;

  const update = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    setCanLeft(el.scrollLeft > 4);
    setCanRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 4);
  }, [ref]);

  useLayoutEffect(() => {
    update();
  }, [update, count, width]);

  if (!extras?.length) return null;
  const scrollPage = (dir: 1 | -1) => ref.current?.scrollBy({ left: dir * perPage * (width + gap), behavior: smoothScroll() });

  return (
    <section className="mt-12">
      <SectionTitle
        action={
          canLeft || canRight ? (
            <div className="flex shrink-0 items-center gap-1">
              <ArrowButton side="left" enabled={canLeft} onClick={() => scrollPage(-1)} />
              <ArrowButton side="right" enabled={canRight} onClick={() => scrollPage(1)} />
            </div>
          ) : null
        }
      >
        {t("extras.title")}
      </SectionTitle>
      <div
        ref={ref}
        onScroll={update}
        className="scrollbar-none -mx-1 flex snap-x snap-mandatory scroll-px-1 gap-3.5 overflow-x-auto px-1 pt-1 pb-2 sm:gap-4"
      >
        {extras.map((extra) => (
          <div key={extra.id} className="shrink-0 snap-start">
            <MediaCard
              item={extra}
              aspect="thumb"
              width={width}
              fixed
              href={playUrl(extra.id)}
              subtitle={extraSubtitle(extra)}
            />
          </div>
        ))}
      </div>
    </section>
  );
}
