import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router";
import { Layers, Plus, Tags } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useCategories, useCollections, type Category, type CategoryRef, type Collection } from "@/api/lists";
import type { Id, LibraryKind } from "@/api/types";
import { useAuth } from "@/auth";
import { ItemGridSkeleton } from "@/components/ItemGrid";
import { CollectionCard, collectionNoun } from "@/components/lists/CollectionCard";
import { Button, EmptyState, Select } from "@/components/ui";
import { useElementWidth } from "@/lib/hooks";
import { decodeId, encodeId } from "@/lib/ids";
import { CategoriesModal } from "./CategoriesModal";
import { CollectionFormModal } from "./CollectionFormModal";
import { collectionUrl } from "@/lib/urls";

const CARD_WIDTH = 160;

export type CollectionType = "all" | "manual" | "smart" | "tmdb";
const TYPES: CollectionType[] = ["all", "manual", "smart", "tmdb"];

/** Whether a collection is of `type`: made by hand, filled by a filter, or TMDB's. */
export function isOfType(c: Collection, type: CollectionType): boolean {
  switch (type) {
    case "all":
      return true;
    case "smart":
      return c.smart_filter != null;
    case "tmdb":
      return c.automatic;
    case "manual":
      return c.smart_filter == null && !c.automatic;
  }
}

/** The category filter: every one, or one category's id. */
type CategoryChoice = "all" | Id;

/**
 * The filters are in the address (`type`, `category`), so that Back from a
 * collection finds them as they were.
 */
function filtersOf(params: URLSearchParams): { type: CollectionType; category: CategoryChoice } {
  const type = params.get("type");
  const category = params.get("category");
  return {
    type: TYPES.find((t) => t === type) ?? "all",
    category: decodeId(category) ?? "all",
  };
}

export interface CollectionGroup {
  category: CategoryRef;
  collections: Collection[];
}

/** `collections` by the category the server gives each (Other for those
 * without one), in the categories' order; empty ones left out. */
export function groupByCategory(collections: Collection[], categories: Category[]): CollectionGroup[] {
  const groups = new Map<Id, CollectionGroup>(categories.map((category) => [category.id, { category, collections: [] }]));
  for (const c of collections) {
    // One of a category not listed yet (made since) comes after the rest.
    const group = groups.get(c.category.id) ?? { category: c.category, collections: [] };
    groups.set(c.category.id, group);
    group.collections.push(c);
  }
  return [...groups.values()].filter((g) => g.collections.length > 0);
}

/** The "Collections" tab of a movie or TV library: its collections by category. */
export function CollectionsTab({ libraryId, kind }: { libraryId: Id; kind: LibraryKind }) {
  const { t } = useTranslation(["item", "common"]);
  const user = useAuth();
  const navigate = useNavigate();
  const collections = useCollections(libraryId);
  const categories = useCategories();
  const [creating, setCreating] = useState(false);
  const [sorting, setSorting] = useState(false);
  const [params, setParams] = useSearchParams();
  const { type: chosenType, category: chosenCategory } = filtersOf(params);
  const choose = (filters: { type?: CollectionType; category?: CategoryChoice }) => {
    const next = new URLSearchParams(params);
    if (filters.type) {
      if (filters.type === "all") next.delete("type");
      else next.set("type", filters.type);
    }
    if (filters.category) {
      if (filters.category === "all") next.delete("category");
      else next.set("category", encodeId(filters.category));
    }
    setParams(next, { replace: true });
  };
  const [ref, width] = useElementWidth<HTMLDivElement>();
  const gap = width > 0 && width < 640 ? 12 : 18;
  const cols = width > 0 ? Math.max(2, Math.floor((width + gap) / (CARD_WIDTH + gap))) : 6;
  const colWidth = width > 0 ? (width - gap * (cols - 1)) / cols : CARD_WIDTH;
  const noun = collectionNoun(kind);
  const list = collections.data ?? [];
  // How a collection was made matters only to whoever manages them.
  const type = user.is_admin ? chosenType : "all";
  const shown = list.filter((c) => isOfType(c, type));
  // Offered when there is more than one to choose between.
  const usedCategories = groupByCategory(list, categories.data ?? []);
  // A category deleted or emptied since it was chosen filters nothing.
  const category =
    chosenCategory === "all" || usedCategories.some((g) => g.category.id === chosenCategory) ? chosenCategory : "all";
  const groups = groupByCategory(shown, categories.data ?? []).filter((g) => category === "all" || g.category.id === category);
  const count = groups.reduce((n, g) => n + g.collections.length, 0);
  // One group shown needs no heading: the Category filter, when there is
  // one, says which it is.
  const headed = groups.length > 1;
  const filtered = type !== "all" || category !== "all";
  const newButton = (variant: "primary" | "secondary") => (
    <Button variant={variant} size={variant === "primary" ? "md" : "sm"} icon={<Plus className="size-4" />} onClick={() => setCreating(true)}>
      {t("collectionForm.new")}
    </Button>
  );

  return (
    <div className="px-4 pt-5 sm:px-8">
      <div className="mb-4 flex min-h-8 flex-wrap items-center gap-2">
        {list.length > 0 ? (
          <>
            {user.is_admin ? (
              <Select aria-label={t("collections.type")} value={type} onChange={(e) => choose({ type: e.target.value as CollectionType })}>
                {TYPES.map((v) => (
                  <option key={v} value={v}>
                    {t(`collections.types.${v}`)}
                  </option>
                ))}
              </Select>
            ) : null}
            {usedCategories.length > 1 || category !== "all" ? (
              <Select
                aria-label={t("collections.category")}
                value={String(category)}
                onChange={(e) => {
                  const v = e.target.value;
                  choose({ category: v === "all" ? v : Number(v) });
                }}
              >
                <option value="all">{t("collections.allCategories")}</option>
                {usedCategories.map((g) => (
                  <option key={g.category.id} value={String(g.category.id)}>
                    {g.category.title}
                  </option>
                ))}
              </Select>
            ) : null}
            {filtered ? (
              <Button size="sm" variant="ghost" onClick={() => choose({ type: "all", category: "all" })}>
                {t("collections.clearFilters")}
              </Button>
            ) : null}
          </>
        ) : null}
        <span className="text-sm tabular-nums text-muted">{collections.data ? t("counts.collections", { count }) : ""}</span>
        {user.is_admin && list.length > 0 ? (
          <div className="ml-auto flex items-center gap-2">
            <Button size="sm" variant="ghost" icon={<Tags className="size-4" />} onClick={() => setSorting(true)}>
              {t("categories.manage")}
            </Button>
            {newButton("secondary")}
          </div>
        ) : null}
      </div>
      <div ref={ref}>
        {/* Without the categories every collection would show under Other
            for a moment; if they fail, it does. */}
        {collections.isPending || categories.isPending ? (
          <ItemGridSkeleton aspect="poster" cardWidth={CARD_WIDTH} />
        ) : collections.isError ? (
          <EmptyState
            title={t("collections.loadError")}
            description={errorMessage(collections.error)}
            action={<Button onClick={() => void collections.refetch()}>{t("common:actions.retry")}</Button>}
          />
        ) : list.length === 0 ? (
          <EmptyState
            icon={<Layers />}
            title={t("collections.empty")}
            description={
              kind === "movies"
                ? t(user.is_admin ? "collections.moviesHintAdmin" : "collections.moviesHint")
                : user.is_admin
                  ? t("collections.showsHintAdmin")
                  : t("collections.noneHere")
            }
            action={user.is_admin ? newButton("primary") : undefined}
          />
        ) : count === 0 ? (
          <EmptyState
            title={t("collections.noMatches")}
            action={<Button onClick={() => choose({ type: "all", category: "all" })}>{t("collections.clearFilters")}</Button>}
          />
        ) : (
          <div className="space-y-10">
            {groups.map((g) => (
              <section key={g.category.id} aria-label={headed ? g.category.title : undefined}>
                {headed ? (
                  <h2 className="mb-3 flex items-baseline gap-2 text-lg font-semibold tracking-tight">
                    {g.category.title}
                    <span className="text-sm font-normal tabular-nums text-muted">{g.collections.length}</span>
                  </h2>
                ) : null}
                <div
                  className="grid"
                  style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`, columnGap: gap, rowGap: gap < 18 ? 18 : 26 }}
                >
                  {g.collections.map((c) => (
                    <CollectionCard key={c.id} collection={c} width={colWidth} noun={noun} />
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}
      </div>
      {user.is_admin ? (
        <>
          <CollectionFormModal
            key={creating ? "open" : "closed"}
            open={creating}
            onClose={() => setCreating(false)}
            libraryId={libraryId}
            onCreated={(c) => void navigate(collectionUrl(c.id))}
          />
          <CategoriesModal open={sorting} onClose={() => setSorting(false)} />
        </>
      ) : null}
    </div>
  );
}
