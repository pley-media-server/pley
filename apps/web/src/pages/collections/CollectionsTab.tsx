import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Layers, Plus } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useCollections } from "@/api/lists";
import type { Id, LibraryKind } from "@/api/types";
import { useAuth } from "@/auth";
import { ItemGridSkeleton } from "@/components/ItemGrid";
import { CollectionCard, collectionNoun } from "@/components/lists/CollectionCard";
import { Button, EmptyState } from "@/components/ui";
import { useElementWidth } from "@/lib/hooks";
import { CollectionFormModal } from "./CollectionFormModal";
import { collectionUrl } from "@/lib/urls";

const CARD_WIDTH = 160;

/** The "Collections" tab of a movie or TV library. */
export function CollectionsTab({ libraryId, kind }: { libraryId: Id; kind: LibraryKind }) {
  const { t } = useTranslation(["item", "common"]);
  const user = useAuth();
  const navigate = useNavigate();
  const collections = useCollections(libraryId);
  const [creating, setCreating] = useState(false);
  const [ref, width] = useElementWidth<HTMLDivElement>();
  const gap = width > 0 && width < 640 ? 12 : 18;
  const cols = width > 0 ? Math.max(2, Math.floor((width + gap) / (CARD_WIDTH + gap))) : 6;
  const colWidth = width > 0 ? (width - gap * (cols - 1)) / cols : CARD_WIDTH;
  const noun = collectionNoun(kind);
  const list = collections.data ?? [];
  const newButton = (variant: "primary" | "secondary") => (
    <Button variant={variant} size={variant === "primary" ? "md" : "sm"} icon={<Plus className="size-4" />} onClick={() => setCreating(true)}>
      {t("collectionForm.new")}
    </Button>
  );

  return (
    <div className="px-4 pt-5 sm:px-8">
      <div className="mb-4 flex min-h-8 items-center justify-between gap-3">
        <span className="text-sm tabular-nums text-muted">{collections.data ? t("counts.collections", { count: list.length }) : ""}</span>
        {user.is_admin && list.length > 0 ? newButton("secondary") : null}
      </div>
      <div ref={ref}>
        {collections.isPending ? (
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
        ) : (
          <div
            className="grid"
            style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`, columnGap: gap, rowGap: gap < 18 ? 18 : 26 }}
          >
            {list.map((c) => (
              <CollectionCard key={c.id} collection={c} width={colWidth} noun={noun} />
            ))}
          </div>
        )}
      </div>
      {user.is_admin ? (
        <CollectionFormModal
          key={creating ? "open" : "closed"}
          open={creating}
          onClose={() => setCreating(false)}
          libraryId={libraryId}
          onCreated={(c) => void navigate(collectionUrl(c.id))}
        />
      ) : null}
    </div>
  );
}
