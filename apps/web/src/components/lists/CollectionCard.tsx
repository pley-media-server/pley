import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import type { Collection } from "@/api/lists";
import type { LibraryKind } from "@/api/types";
import { Artwork } from "@/components/Artwork";
import { collectionUrl } from "@/lib/urls";

/** What a collection of this library holds: ["movie", "movies"] or ["show", "shows"]. */
export function collectionNoun(kind: LibraryKind | undefined): [string, string] {
  return kind === "shows" ? ["show", "shows"] : ["movie", "movies"];
}

export function CollectionCard({
  collection,
  width,
  noun,
}: {
  collection: Collection;
  width: number;
  noun: [string, string];
}) {
  const { t } = useTranslation("components");
  const to = collectionUrl(collection.id);
  const count = collection.item_count;
  return (
    <div className="group/card min-w-0">
      <Link
        to={to}
        aria-label={collection.title}
        className="relative block overflow-hidden rounded-md bg-surface-2 outline-none ring-1 ring-white/5 transition duration-200 hover:ring-2 hover:ring-accent focus-visible:ring-2 focus-visible:ring-accent"
      >
        <Artwork
          candidates={[collection.poster_url]}
          width={width}
          alt=""
          title={collection.title}
          mark="play"
          className="aspect-[2/3] w-full"
          imgClassName="transition-transform duration-300 motion-safe:group-hover/card:scale-[1.04]"
        />
        <div className="pointer-events-none absolute inset-0 bg-black/0 transition-colors duration-200 group-hover/card:bg-black/30" />
        <span className="absolute right-1.5 top-1.5 min-w-6 rounded bg-black/75 px-1.5 py-0.5 text-center text-xs font-bold text-fg shadow">
          {collection.item_count}
        </span>
      </Link>
      <div className="mt-2 px-0.5">
        <Link to={to} tabIndex={-1} className="block truncate text-sm font-medium text-fg transition-colors hover:text-accent" title={collection.title}>
          {collection.title}
        </Link>
        <div className="truncate text-xs text-muted">
          {/* `noun` says what the library holds; the count is in the reader's language. */}
          {noun[1] === "shows" ? t("collectionCard.shows", { count }) : t("collectionCard.movies", { count })}
        </div>
      </div>
    </div>
  );
}
