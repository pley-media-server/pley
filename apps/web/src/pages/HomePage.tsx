import { useMemo, useState } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { Clapperboard, FolderPlus, Rows3 } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useHomeHubs, useLibraries } from "@/api/hooks";
import { useAuth } from "@/auth";
import { HubRow, HubRowSkeleton } from "@/components/HubRow";
import { Button, buttonClass, EmptyState } from "@/components/ui";
import { cn } from "@/lib/cn";
import { pickFeatured } from "@/lib/featured";
import { useDocumentTitle } from "@/lib/hooks";
import { HomeBanner, HomeBannerSkeleton, ROWS_OVERLAP } from "./HomeBanner";
import { HomeProgress } from "./HomeProgress";
import { HomeRowsModal } from "./collections/RowsModal";

export function HomePage() {
  const { t } = useTranslation(["pages", "common"]);
  useDocumentTitle(t("home.title"));
  const user = useAuth();
  const libs = useLibraries();
  const hubs = useHomeHubs();
  const featured = useMemo(() => pickFeatured(hubs.data ?? []), [hubs.data]);
  const [arranging, setArranging] = useState(false);

  if (libs.data && libs.data.length === 0) {
    return (
      <EmptyState
        className="min-h-[70vh]"
        icon={<FolderPlus />}
        title={t("home.noLibraries")}
        description={user.is_admin ? t("home.noLibrariesAdmin") : t("home.noLibrariesUser")}
        action={
          user.is_admin ? (
            <Link to="/settings/libraries?add=1" className={buttonClass("primary", "lg")}>
              {t("home.addLibrary")}
            </Link>
          ) : undefined
        }
      />
    );
  }

  if (hubs.isPending) {
    return (
      <>
        <HomeBannerSkeleton />
        <div className={cn("relative space-y-9 pb-7 sm:space-y-10 sm:pb-8", ROWS_OVERLAP)}>
          <HubRowSkeleton aspect="thumb" />
          <HubRowSkeleton />
        </div>
      </>
    );
  }

  if (hubs.isError) {
    return (
      <EmptyState
        className="min-h-[60vh]"
        title={t("home.cantLoad")}
        description={errorMessage(hubs.error)}
        action={<Button onClick={() => void hubs.refetch()}>{t("common:actions.retry")}</Button>}
      />
    );
  }

  const visible = hubs.data.filter((h) => h.items.length > 0);
  const arrange = user.is_admin ? (
    <>
      <Button size="sm" variant="ghost" icon={<Rows3 className="size-4" />} onClick={() => setArranging(true)}>
        {t("home.arrangeRows")}
      </Button>
      <HomeRowsModal open={arranging} onClose={() => setArranging(false)} />
    </>
  ) : null;
  if (visible.length === 0) {
    const scanning = libs.data?.some((l) => l.scanning);
    // Libraries with titles in them and still nothing: administrators took
    // every row off Home.
    const stocked = !scanning && libs.data?.some((l) => l.item_count > 0);
    return (
      <>
        <HomeProgress className="mx-4 mt-6 sm:mx-8" />
        <EmptyState
          className="min-h-[70vh]"
          icon={<Clapperboard />}
          title={scanning ? t("home.scanning") : stocked ? t("home.noRows") : t("home.empty")}
          description={
            scanning
              ? t("home.scanningHint")
              : stocked
                ? t(user.is_admin ? "home.noRowsHintAdmin" : "home.noRowsHint")
                : t("home.emptyHint")
          }
          action={
            stocked ? (
              arrange
            ) : user.is_admin && !scanning ? (
              <Link to="/settings/libraries" className={buttonClass()}>
                {t("home.manage")}
              </Link>
            ) : undefined
          }
        />
      </>
    );
  }

  return (
    <>
      {/* What's filling in comes first while it shows: the banner then starts below it rather than under the top bar. */}
      <div data-home-progress className="px-4 pt-6 pb-6 empty:hidden sm:px-8">
        <HomeProgress />
      </div>
      {featured.length ? <HomeBanner featured={featured} /> : null}
      <div className={cn("relative space-y-9 py-7 sm:space-y-10 sm:py-8", featured.length && [ROWS_OVERLAP, "pt-0 sm:pt-0"])}>
        {visible.map((hub) => (
          <HubRow key={hub.key} hub={hub} />
        ))}
        {arrange ? <div className="flex justify-center px-4 sm:px-8">{arrange}</div> : null}
      </div>
    </>
  );
}
