import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useSaveSettings, useSettings } from "@/api/hooks";
import { useToast } from "@/components/Toast";
import { Card, Toggle } from "@/components/ui";
import { Row } from "./SettingsForm";

/**
 * Whether Pley casts to the Google Cast and AirPlay devices on its network
 * (Settings → Network): off, the household's Play on… lists none and Pley
 * doesn't look for them. It saves at once.
 */
export function CastDevices() {
  const { t } = useTranslation(["settings", "common"]);
  const settings = useSettings();
  const save = useSaveSettings();
  const toast = useToast();
  const on = settings.data?.cast_devices;
  // A server from before the setting.
  if (on === undefined) return null;

  return (
    <Card>
      <Row label={t("castDevices.label")} hint={t("castDevices.hint")}>
        <Toggle
          checked={on}
          disabled={save.isPending}
          onChange={(v) =>
            save.mutate(
              { cast_devices: v },
              {
                onSuccess: () => toast.success(t("common:states.saved")),
                onError: (e) => toast.error(errorMessage(e)),
              },
            )
          }
          label={t("castDevices.label")}
        />
      </Row>
    </Card>
  );
}
