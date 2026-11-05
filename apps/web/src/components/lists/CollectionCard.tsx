import { useLayoutEffect, useRef, useState } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { useCollections, type Collection } from "@/api/lists";
import type { Id, LibraryKind } from "@/api/types";
import { Artwork } from "@/components/Artwork";
import { fitCards } from "@/components/HubRow";
import { useMediaQuery } from "@/lib/hooks";
import { collectionUrl, libraryUrl } from "@/lib/urls";

/** What a collection of this library holds: ["movie", "movies"] or ["show", "shows"]. */
export function collectionNoun(kind: LibraryKind | undefined): [string, string] {
  return kind === "shows" ? ["show", "shows"] : ["movie", "movies"];
}

export function CollectionCard({
  collection,
  width,
  noun,
  fixed,
}: {
  collection: Collection;
  width: number;
  noun: [string, string];
  fixed?: boolean;
}) {
  const { t } = useTranslation("components");
  const to = collectionUrl(collection.id);
  const count = collection.item_count;
  return (
    <div className="group/card min-w-0" style={fixed ? { width } : undefined}>
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

/** "Collections" shelf of a library's Recommended tab. */
export function CollectionsRow({ libraryId, kind }: { libraryId: Id; kind: LibraryKind }) {
  const { t } = useTranslation("components");
  const collections = useCollections(libraryId, kind !== "music");
  const desktop = useMediaQuery("(min-width: 640px)");
  const ref = useRef<HTMLDivElement>(null);
  const [available, setAvailable] = useState(0);
  const list = collections.data ?? [];
  const hasCollections = list.length > 0;

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const style = getComputedStyle(el);
      setAvailable(el.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight));
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [hasCollections]);

  if (!hasCollections) return null;
  const gap = desktop ? 16 : 14;
  const { width } = available > 0 ? fitCards(available, desktop ? 160 : 118, gap) : { width: desktop ? 160 : 118 };
  const noun = collectionNoun(kind);

  return (
    <section>
      <div className="mb-2 flex items-center justify-between gap-4 px-4 sm:px-8">
        <h2 className="truncate text-lg font-semibold tracking-tight">
          <Link to={libraryUrl(libraryId, "tab=collections")} className="transition-colors hover:text-accent">
            {t("collectionCard.collections")}
          </Link>
        </h2>
        <Link to={libraryUrl(libraryId, "tab=collections")} className="shrink-0 text-xs font-medium text-muted transition-colors hover:text-accent">
          {t("hubRow.seeAll")}
        </Link>
      </div>
      <div className="px-4 sm:px-8">
        <div ref={ref} className="scrollbar-none -mx-1 flex snap-x snap-mandatory scroll-px-1 gap-3.5 overflow-x-auto px-1 pb-2 pt-1 sm:gap-4">
          {list.map((c) => (
            <div key={c.id} className="shrink-0 snap-start">
              <CollectionCard collection={c} width={width} noun={noun} fixed />
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
