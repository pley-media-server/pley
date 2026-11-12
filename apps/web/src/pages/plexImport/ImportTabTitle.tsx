import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { usePlexImport, type ImportState } from "@/api/plexImport";
import { useAuth } from "@/auth";
import { useTitleNote } from "@/lib/hooks";
import { formatShare, importShare } from "./share";

/**
 * The Plex import in the tab's title, to administrators, whatever the page:
 * how far it is while it runs and, when it ends while the tab is out of
 * sight, how it ended, until the tab is seen again.
 */
export function ImportTabTitle() {
  const user = useAuth();
  return user.is_admin ? <AdminImportTitle /> : null;
}

function AdminImportTitle() {
  const { t } = useTranslation("common");
  const data = usePlexImport().data;
  const state = data?.state;
  const [seen, setSeen] = useState(state);
  const [ended, setEnded] = useState<ImportState | null>(null);
  if (state !== seen) {
    setSeen(state);
    if (seen === "running" && state && state !== "running" && state !== "idle" && document.hidden) setEnded(state);
  }

  useEffect(() => {
    if (!ended) return;
    const seenAgain = () => {
      if (!document.hidden) setEnded(null);
    };
    document.addEventListener("visibilitychange", seenAgain);
    return () => document.removeEventListener("visibilitychange", seenAgain);
  }, [ended]);

  const note =
    data?.state === "running"
      ? t("importTitle.running", { percent: formatShare(importShare(data)) })
      : ended && ended !== "running" && ended !== "idle"
        ? t(`importTitle.${ended}`)
        : null;
  useTitleNote(note);
  return null;
}
