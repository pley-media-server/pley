import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { ArrowRight, FolderPlus, Import } from "lucide-react";
import { useDocumentTitle } from "@/lib/hooks";
import { AuthLayout } from "./AuthLayout";

/**
 * After Your server (`/setup/start`): bring over another server (Plex), or
 * start fresh. The question names no server, so others can join Plex here.
 */
export function SetupStartPage() {
  const { t } = useTranslation("pages");
  useDocumentTitle(t("setupStart.title"));
  const choice =
    "group flex flex-col rounded-xl border border-border bg-surface/90 p-6 text-left shadow-2xl shadow-black/50 backdrop-blur transition-colors hover:border-accent hover:bg-accent/5 focus-visible:border-accent focus-visible:outline-none";
  return (
    <AuthLayout title={t("setupStart.title")} wide bare>
      <div className="grid gap-3 sm:grid-cols-2">
        <Link to="/setup/plex" className={choice}>
          <span className="flex items-center justify-between">
            <Import className="size-6 text-accent" />
            <ArrowRight className="size-4 text-subtle group-hover:text-accent" />
          </span>
          <span className="mt-4 block font-medium">{t("setupStart.fromPlex")}</span>
          <span className="mt-1 block text-xs text-muted">{t("setupStart.fromPlexHint")}</span>
        </Link>
        <Link to="/setup/library" className={choice}>
          <span className="flex items-center justify-between">
            <FolderPlus className="size-6 text-accent" />
            <ArrowRight className="size-4 text-subtle group-hover:text-accent" />
          </span>
          <span className="mt-4 block font-medium">{t("setupStart.fresh")}</span>
          <span className="mt-1 block text-xs text-muted">{t("setupStart.freshHint")}</span>
        </Link>
      </div>
    </AuthLayout>
  );
}
