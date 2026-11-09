import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { ArrowRight, FolderPlus, Import } from "lucide-react";
import { useDocumentTitle } from "@/lib/hooks";
import { AuthLayout } from "./AuthLayout";

/** After the administrator's account is made (`/setup/start`): from Plex, or from scratch. */
export function SetupStartPage() {
  const { t } = useTranslation("pages");
  useDocumentTitle(t("setupStart.title"));
  const choice =
    "group flex items-center gap-3 rounded-lg border border-border bg-surface-2 p-4 text-left transition-colors hover:border-accent hover:bg-accent/5 focus-visible:border-accent focus-visible:outline-none";
  return (
    <AuthLayout title={t("setupStart.title")} subtitle={t("setupStart.subtitle")}>
      <div className="space-y-3">
        <Link to="/setup/plex" className={choice}>
          <Import className="size-5 shrink-0 text-accent" />
          <span className="min-w-0 flex-1">
            <span className="block font-medium">{t("setupStart.fromPlex")}</span>
            <span className="block text-xs text-muted">{t("setupStart.fromPlexHint")}</span>
          </span>
          <ArrowRight className="size-4 shrink-0 text-subtle group-hover:text-accent" />
        </Link>
        <Link to="/setup/library" className={choice}>
          <FolderPlus className="size-5 shrink-0 text-accent" />
          <span className="min-w-0 flex-1">
            <span className="block font-medium">{t("setupStart.fresh")}</span>
            <span className="block text-xs text-muted">{t("setupStart.freshHint")}</span>
          </span>
          <ArrowRight className="size-4 shrink-0 text-subtle group-hover:text-accent" />
        </Link>
      </div>
    </AuthLayout>
  );
}
