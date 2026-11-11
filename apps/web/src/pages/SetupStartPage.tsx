import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router";
import { ArrowRight, FolderPlus, Import } from "lucide-react";
import { Button } from "@/components/ui";
import { useDocumentTitle } from "@/lib/hooks";
import { SETUP_PROGRESS } from "@/lib/setupFrame";
import { SetupActions, SetupStep } from "./SetupLayout";

/**
 * After Your server (`/setup/start`): bring over another server (Plex), or
 * start fresh. The question names no server, so others can join Plex here.
 */
export function SetupStartPage() {
  const { t } = useTranslation(["pages", "common"]);
  useDocumentTitle(t("setupStart.title"));
  const navigate = useNavigate();
  const choice =
    "group flex flex-col rounded-lg border border-border bg-surface-2 p-5 text-left transition-colors hover:border-accent hover:bg-accent/5 focus-visible:border-accent focus-visible:outline-none";
  return (
    <SetupStep progress={SETUP_PROGRESS.start} title={t("setupStart.title")}>
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
      <SetupActions
        back={
          <Button variant="ghost" onClick={() => void navigate("/setup/server", { replace: true })}>
            {t("common:actions.back")}
          </Button>
        }
      />
    </SetupStep>
  );
}
