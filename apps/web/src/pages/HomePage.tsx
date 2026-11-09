import { useMemo } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { Clapperboard, FolderPlus } from "lucide-react";
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

export function HomePage() {
  const { t } = useTranslation(["pages", "common"]);
  useDocumentTitle(t("home.title"));
  const user = useAuth();
  const libs = useLibraries();
  const hubs = useHomeHubs();
  const featured = useMemo(() => pickFeatured(hubs.data ?? []), [hubs.data]);

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
  if (visible.length === 0) {
    const scanning = libs.data?.some((l) => l.scanning);
    return (
      <>
        <HomeProgress className="mx-4 mt-6 sm:mx-8" />
        <EmptyState
          className="min-h-[70vh]"
          icon={<Clapperboard />}
          title={scanning ? t("home.scanning") : t("home.empty")}
          description={scanning ? t("home.scanningHint") : t("home.emptyHint")}
          action={
            user.is_admin && !scanning ? (
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
      {featured.length ? <HomeBanner featured={featured} /> : null}
      <div className={cn("relative space-y-9 py-7 sm:space-y-10 sm:py-8", featured.length && [ROWS_OVERLAP, "pt-0 sm:pt-0"])}>
        <HomeProgress className="mx-4 sm:mx-8" />
        {visible.map((hub) => (
          <HubRow key={hub.key} hub={hub} />
        ))}
      </div>
    </>
  );
}
