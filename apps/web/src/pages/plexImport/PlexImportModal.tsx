import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Modal } from "@/components/Modal";
import { PlexImport } from "./PlexImport";

/** Settings › Libraries › Import from Plex: the same steps as the first run's, or the last import's report. */
export function PlexImportModal({ open, onClose, report = false }: { open: boolean; onClose: () => void; report?: boolean }) {
  const { t } = useTranslation(["settings", "common"]);
  const [busy, setBusy] = useState(false);
  return (
    <Modal open={open} onClose={onClose} locked={busy} size="xl" title={t("plexImport.title")}>
      {open ? (
        <PlexImport
          onLeave={onClose}
          onFinish={onClose}
          leaveLabel={t("common:actions.cancel")}
          goOnLabel={t("common:actions.close")}
          report={report}
          onBusy={setBusy}
        />
      ) : null}
    </Modal>
  );
}
