import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { RequireAdmin } from "@/auth";
import { useDocumentTitle } from "@/lib/hooks";
import { AuthLayout } from "../AuthLayout";
import { PlexImport } from "./PlexImport";

/** The first run's way in from Plex (`/setup/plex`), once the administrator's account is made. */
export function PlexSetupPage() {
  const { t } = useTranslation("settings");
  useDocumentTitle(t("plexImport.setupTitle"));
  const navigate = useNavigate();
  return (
    <AuthLayout title={t("plexImport.setupTitle")} wide>
      <RequireAdmin>
        <PlexImport
          leaveLabel={t("plexImport.startFresh")}
          onLeave={() => void navigate("/settings/libraries?add=1", { replace: true })}
          onFinish={() => void navigate("/", { replace: true })}
        />
      </RequireAdmin>
    </AuthLayout>
  );
}
