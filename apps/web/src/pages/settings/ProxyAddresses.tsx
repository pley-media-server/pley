import { useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { CircleAlert, Plus, X } from "lucide-react";
import { api, errorMessage } from "@/api/client";
import { useSaveSettings, useSettings } from "@/api/hooks";
import { useToast } from "@/components/Toast";
import { Button, Card, IconButton, TextInput } from "@/components/ui";

/**
 * Whether pley took `here` (the page's own address) for `seen`, the same
 * address without its port: a reverse proxy passed the address on without
 * it (nginx's `$host`, Nginx Proxy Manager's default).
 */
export function droppedPort(here: string, seen: string): boolean {
  try {
    const page = new URL(here);
    const server = new URL(seen);
    return page.port !== "" && server.port === "" && page.hostname === server.hostname && page.protocol === server.protocol;
  } catch {
    return false;
  }
}

/**
 * The addresses pley is reached at through a reverse proxy that leaves the
 * port out (Settings → Network). pley names itself by the address a request
 * came to, and apps refuse a server naming another than the one they used:
 * for these, pley puts the port back. The page notices when it was opened at
 * such an address, and offers to add it.
 */
export function ProxyAddresses() {
  const { t } = useTranslation(["settings", "common"]);
  const settings = useSettings();
  const save = useSaveSettings();
  const toast = useToast();
  const [draft, setDraft] = useState("");
  // The address pley takes this page's to be: its sign-in issuer.
  const seen = useQuery({
    queryKey: ["issuer"],
    queryFn: ({ signal }) => api.get<{ issuer: string }>("/.well-known/oauth-authorization-server", { signal }),
    retry: false,
  });
  const addresses = settings.data?.proxy_addresses;
  // A server from before the setting.
  if (!addresses) return null;

  const here = window.location.origin;
  const hidden = seen.data && droppedPort(here, seen.data.issuer) ? seen.data.issuer : null;
  const saveAddresses = (next: string[], done?: () => void) =>
    save.mutate(
      { proxy_addresses: next },
      {
        onSuccess: () => {
          toast.success(t("common:states.saved"));
          void seen.refetch();
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
        <h3 className="text-base font-semibold">{t("proxy.title")}</h3>
        <p className="mt-0.5 text-sm text-muted">
          <Trans t={t} i18nKey="proxy.description" components={{ code: <code className="font-mono text-xs" /> }} />
        </p>
      </div>
      {hidden ? (
        <div role="alert" className="space-y-2 rounded-md border border-danger/40 bg-danger/10 p-3 text-sm">
          <p className="flex items-start gap-2">
            <CircleAlert className="mt-0.5 size-4 shrink-0 text-danger" />
            <span>
              <Trans
                t={t}
                i18nKey="proxy.openedAt"
                values={{ here, seen: hidden }}
                components={{ address: <span className="font-mono text-xs" /> }}
              />
            </span>
          </p>
          <Button size="sm" variant="primary" loading={save.isPending} onClick={() => saveAddresses([...addresses, here])}>
            {t("proxy.add", { address: here })}
          </Button>
        </div>
      ) : null}
      {addresses.length ? (
        <ul className="space-y-1 text-sm">
          {addresses.map((address) => (
            <li key={address} className="flex items-center justify-between gap-2">
              <span className="truncate font-mono text-xs">{address}</span>
              <IconButton
                size="sm"
                label={t("proxy.remove", { address })}
                disabled={save.isPending}
                onClick={() => saveAddresses(addresses.filter((a) => a !== address))}
              >
                <X className="size-4" />
              </IconButton>
            </li>
          ))}
        </ul>
      ) : null}
      <div className="flex gap-2">
        <TextInput
          aria-label={t("proxy.address")}
          placeholder="https://media.example.com:9999"
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
