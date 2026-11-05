import { Fragment, useCallback, useLayoutEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import type { Credit, CreditRole } from "@/api/types";
import { ArrowButton, useFittedRow } from "@/components/HubRow";
import { PersonCard } from "@/components/PersonCard";
import { SectionTitle } from "@/components/ui";
import { smoothScroll } from "@/lib/hooks";
import { personUrl } from "@/lib/urls";

/** Smallest card width: [mobile, desktop]; cards grow to fill whole rows. */
const CARD_WIDTHS: [number, number] = [96, 120];

/** Scrolling row of people (the cast of a movie or show, an episode's guests). */
export function CastRow({ title, credits }: { title: string; credits?: Credit[] }) {
  const { ref, width, perPage, gap } = useFittedRow(CARD_WIDTHS);
  const [canLeft, setCanLeft] = useState(false);
  const [canRight, setCanRight] = useState(false);
  const count = credits?.length ?? 0;

  const update = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    setCanLeft(el.scrollLeft > 4);
    setCanRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 4);
  }, [ref]);

  useLayoutEffect(() => {
    update();
  }, [update, count, width]);

  if (!credits?.length) return null;
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
        {title}
      </SectionTitle>
      <div
        ref={ref}
        onScroll={update}
        className="scrollbar-none -mx-1 flex snap-x snap-mandatory scroll-px-1 gap-3.5 overflow-x-auto px-1 pt-1 pb-2 sm:gap-4"
      >
        {credits.map((c) => (
          <div key={`${c.role}-${c.id}`} className="shrink-0 snap-start">
            <PersonCard person={c} subtitle={c.character} width={width} />
          </div>
        ))}
      </div>
    </section>
  );
}

function PeopleLinks({ people, max = 4 }: { people: Credit[]; max?: number }) {
  const { t } = useTranslation("item");
  return (
    <>
      {people.slice(0, max).map((p, i) => (
        <Fragment key={p.id}>
          {i > 0 ? ", " : null}
          <Link to={personUrl(p.id)} className="transition-colors hover:text-accent">
            {p.name}
          </Link>
        </Fragment>
      ))}
      {people.length > max ? <span className="text-muted"> {t("crew.more", { count: people.length - max })}</span> : null}
    </>
  );
}

const CREW_ROLES = ["creator", "director", "writer"] as const satisfies CreditRole[];

export function hasCrew(crew?: Credit[]): boolean {
  return !!crew?.some((c) => (CREW_ROLES as readonly CreditRole[]).includes(c.role));
}

/** `<dt>`/`<dd>` pairs for a details list: who created, directed and wrote it. */
export function CrewRows({ crew }: { crew?: Credit[] }) {
  const { t } = useTranslation("item");
  return CREW_ROLES.map((role) => {
    const people = crew?.filter((c) => c.role === role) ?? [];
    if (!people.length) return null;
    return (
      <Fragment key={role}>
        <dt className="text-subtle">{t(`crew.${role}`, { count: people.length })}</dt>
        <dd className="text-fg/85">
          <PeopleLinks people={people} />
        </dd>
      </Fragment>
    );
  });
}
