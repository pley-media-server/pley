import { useTranslation } from "react-i18next";
import { useRelated } from "@/api/hooks";
import type { ItemSummary } from "@/api/types";
import { HubRow } from "@/components/HubRow";

/** Movies or shows like this one, when there are any. */
export function MoreLikeThis({ item }: { item: ItemSummary }) {
  const { t } = useTranslation("item");
  const related = useRelated(item.id, item.kind === "movie" || item.kind === "show");
  if (!related.data?.length) return null;
  return (
    <div className="mt-12">
      <HubRow inset hub={{ key: `related:${item.id}`, title: t("moreLikeThis"), library_id: null, items: related.data }} />
    </div>
  );
}
