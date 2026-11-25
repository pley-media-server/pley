import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, X } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useSaveSettings, useServerAddresses, useSettings } from "@/api/hooks";
import { useToast } from "@/components/Toast";
import { Badge, Button, Card, IconButton, TextInput } from "@/components/ui";

/** Whether `origin` is this computer's own address, which other devices can't reach. */
export function thisComputer(origin: string): boolean {
  try {
    const host = new URL(origin).hostname.replace(/^\[(.*)\]$/, "$1");
    return host === "localhost" || host.endsWith(".localhost") || host.startsWith("127.") || host === "::1";
  } catch {
    return false;
  }
}

/**
 * The addresses apps reach the server at (Settings → Network): the
 * administrator's, in their order, then those Pley finds at home, which are
 * only shown. Apps switch between them by themselves, sending a token only
 * where the server proves itself. The page offers the address it was opened
 * at, unless that is this computer's own.
 */
export function Addresses() {
  const { t } = useTranslation(["settings", "common"]);
  const settings = useSettings();
  const save = useSaveSettings();
  const toast = useToast();
  const [draft, setDraft] = useState("");
  const addresses = settings.data?.addresses;
  const listed = useServerAddresses(addresses !== undefined);
  // A server from before the setting.
  if (!addresses) return null;

  const found = (listed.data?.addresses ?? []).filter((a) => a.source === "found").map((a) => a.url);
  // As the server writes addresses: the browser leaves out the scheme's own port and the `/`.
  const here = window.location.origin;
  const offerHere = !thisComputer(here) && !addresses.includes(here) && !found.includes(here);
  const saveAddresses = (next: string[], done?: () => void) =>
    save.mutate(
      { addresses: next },
      {
        onSuccess: () => {
          toast.success(t("common:states.saved"));
          done?.();
        },
        onError: (e) => toast.error(errorMessage(e)),
      },
    );
  const add = () => {
    const address = draft.trim();
    if (address) saveAddresses([...addresses, address], () => setDraft(""));
  };

  return (
    <Card className="space-y-3">
      <div>
        <h3 className="text-base font-semibold">{t("addresses.title")}</h3>
        <p className="mt-0.5 text-sm text-muted">{t("addresses.hint")}</p>
      </div>
      {addresses.length || found.length ? (
        <ul className="space-y-1 text-sm">
          {addresses.map((address) => (
            <li key={`set ${address}`} className="flex items-center justify-between gap-2">
              <span className="truncate font-mono text-xs">{address}</span>
              <IconButton
                size="sm"
                label={t("addresses.remove", { address })}
                disabled={save.isPending}
                onClick={() => saveAddresses(addresses.filter((a) => a !== address))}
              >
                <X className="size-4" />
              </IconButton>
            </li>
          ))}
          {found.map((address) => (
            <li key={`found ${address}`} className="flex min-h-8 items-center justify-between gap-2">
              <span className="truncate font-mono text-xs">{address}</span>
              <Badge>{t("addresses.automatic")}</Badge>
            </li>
          ))}
        </ul>
      ) : null}
      {offerHere ? (
        <Button size="sm" icon={<Plus className="size-4" />} disabled={save.isPending} onClick={() => saveAddresses([...addresses, here])}>
          {t("addresses.addHere")}
        </Button>
      ) : null}
      <div className="flex gap-2">
        <TextInput
          aria-label={t("addresses.address")}
          placeholder="https://pley.example.com"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            // Its own: the network settings' form isn't saved by it.
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
          className="min-w-0 flex-1 font-mono text-xs"
        />
        <Button size="sm" icon={<Plus className="size-4" />} disabled={!draft.trim() || save.isPending} onClick={add}>
          {t("common:actions.add")}
        </Button>
      </div>
    </Card>
  );
}
