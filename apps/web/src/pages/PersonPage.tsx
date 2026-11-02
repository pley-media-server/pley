import { useParams } from "react-router";
import { UserRound } from "lucide-react";
import { errorMessage, isApiError } from "@/api/client";
import { usePerson } from "@/api/hooks";
import type { CreditRole, PersonCredit, PersonRole } from "@/api/types";
import { CardSkeleton, MediaCard } from "@/components/cards";
import { PersonPhoto } from "@/components/PersonCard";
import { Button, EmptyState, SectionTitle, Skeleton } from "@/components/ui";
import { pluralize } from "@/lib/format";
import { useDocumentTitle, useElementWidth } from "@/lib/hooks";
import { idParam } from "@/lib/ids";

const ROLE_NAMES: Record<CreditRole, string> = {
  actor: "Actor",
  creator: "Creator",
  director: "Director",
  writer: "Writer",
  producer: "Producer",
};

/** "as Sean Maguire", "Director", "Guest star (3 episodes)", … */
export function roleText(roles: PersonRole[]): string {
  return roles
    .map((r) => {
      const base = r.role === "actor" ? (r.character ? `as ${r.character}` : r.episodes ? "Guest star" : "Actor") : ROLE_NAMES[r.role];
      return r.episodes ? `${base} (${pluralize(r.episodes, "episode")})` : base;
    })
    .join(", ");
}

const CARD_WIDTH = 150;
const GAP = 18;

function CreditGrid({ title, credits }: { title: string; credits: PersonCredit[] }) {
  const [ref, width] = useElementWidth<HTMLDivElement>();
  const cols = width > 0 ? Math.max(2, Math.floor((width + GAP) / (CARD_WIDTH + GAP))) : 6;
  const colWidth = width > 0 ? (width - GAP * (cols - 1)) / cols : CARD_WIDTH;
  return (
    <section>
      <SectionTitle>
        {title} <span className="ml-1 text-sm font-normal text-subtle">{credits.length}</span>
      </SectionTitle>
      <div ref={ref} className="grid gap-x-[18px] gap-y-6" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}>
        {credits.map(({ item, roles }) => (
          <MediaCard
            key={item.id}
            item={item}
            aspect="poster"
            width={colWidth}
            subtitle={[item.year, roleText(roles)].filter(Boolean).join(" · ")}
          />
        ))}
      </div>
    </section>
  );
}

/** An actor's or director's page: what they are in, in this server's libraries. */
export function PersonPage() {
  const params = useParams();
  const person = usePerson(idParam(params.id));
  useDocumentTitle(person.data?.name ?? "Cast & crew");

  if (person.isPending) {
    return (
      <div className="space-y-10 px-4 py-8 sm:px-8">
        <div className="flex flex-col items-center gap-5 sm:flex-row sm:items-end sm:gap-7">
          <Skeleton className="size-36 rounded-full sm:size-44" />
          <div className="space-y-2 pb-1">
            <Skeleton className="h-8 w-56" />
            <Skeleton className="h-4 w-36" />
          </div>
        </div>
        <div className="grid gap-x-[18px] gap-y-6" style={{ gridTemplateColumns: `repeat(auto-fill, minmax(${CARD_WIDTH}px, 1fr))` }}>
          {Array.from({ length: 12 }, (_, i) => (
            <CardSkeleton key={i} aspect="poster" />
          ))}
        </div>
      </div>
    );
  }
  if (person.isError && !isApiError(person.error, 404)) {
    return (
      <EmptyState
        className="min-h-[60vh]"
        title="Couldn't load this person"
        description={errorMessage(person.error)}
        action={<Button onClick={() => void person.refetch()}>Retry</Button>}
      />
    );
  }
  if (person.isError || !person.data) {
    return (
      <EmptyState
        className="min-h-[60vh]"
        icon={<UserRound />}
        title="Person not found"
        description={person.isError ? errorMessage(person.error) : undefined}
      />
    );
  }

  const { name, photo_url, credits } = person.data;
  const movies = credits.filter((c) => c.item.kind === "movie");
  const shows = credits.filter((c) => c.item.kind === "show");
  const counts = [movies.length ? pluralize(movies.length, "movie") : null, shows.length ? pluralize(shows.length, "show") : null];

  return (
    <div className="animate-fade-in space-y-10 px-4 py-8 sm:px-8">
      <header className="flex flex-col items-center gap-5 text-center sm:flex-row sm:items-end sm:gap-7 sm:text-left">
        <PersonPhoto name={name} url={photo_url} size={176} eager className="w-36 shrink-0 shadow-2xl shadow-black/50 sm:w-44" />
        <div className="min-w-0 pb-1">
          <h1 className="text-3xl font-bold tracking-tight text-balance sm:text-4xl">{name}</h1>
          <p className="mt-1.5 text-sm text-muted">{counts.filter(Boolean).join(" · ")} in your libraries</p>
        </div>
      </header>
      {movies.length ? <CreditGrid title="Movies" credits={movies} /> : null}
      {shows.length ? <CreditGrid title="Shows" credits={shows} /> : null}
    </div>
  );
}
