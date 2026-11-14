import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useAirPlayPairings, useForgetAirPlayPairing } from "@/api/hooks";
import { useToast } from "@/components/Toast";
import { Card, IconButton } from "@/components/ui";
import { formatDay } from "@/lib/format";

/**
 * The AirPlay devices the server paired with (Settings → Network), which
 * an administrator can forget: the next cast to one asks for its code again.
 * Nothing shows until one is paired, from "Play on…".
 */
export function AirPlayPairings() {
  const { t } = useTranslation(["settings", "common"]);
  const pairings = useAirPlayPairings();
  const forget = useForgetAirPlayPairing();
  const toast = useToast();
  if (!pairings.data?.length) return null;

  return (
    <Card className="space-y-3">
      <div>
        <h3 className="text-base font-semibold">{t("airplay.title")}</h3>
        <p className="mt-0.5 text-sm text-muted">{t("airplay.description")}</p>
      </div>
      <ul className="space-y-2 text-sm">
        {pairings.data.map((p) => (
          <li key={p.device_id} className="flex items-center justify-between gap-2">
            <div className="min-w-0">
              <div className="truncate">{p.name}</div>
              <div className="text-xs text-subtle">
                {p.paired_by
                  ? t("airplay.pairedBy", { name: p.paired_by, date: formatDay(p.paired_at) })
                  : t("airplay.paired", { date: formatDay(p.paired_at) })}
              </div>
            </div>
            <IconButton
              size="sm"
              label={t("airplay.forget", { device: p.name })}
              disabled={forget.isPending}
              onClick={() =>
                forget.mutate(p.device_id, {
                  onSuccess: () => toast.success(t("airplay.forgotten", { device: p.name })),
                  onError: (e) => toast.error(errorMessage(e)),
                })
              }
            >
              <X className="size-4" />
            </IconButton>
          </li>
        ))}
      </ul>
    </Card>
  );
}
