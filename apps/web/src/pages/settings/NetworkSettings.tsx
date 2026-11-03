import { useState, type ReactNode } from "react";
import { Check, CircleAlert, CircleCheck, Info, Lock, Save } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useNetwork, useSaveNetwork } from "@/api/hooks";
import type { ListenChoice, ListenerKind, NetworkAdapter, NetworkSettings as Network } from "@/api/types";
import { useToast } from "@/components/Toast";
import { Badge, Button, Card, EmptyState, PageSpinner, TextInput, Toggle } from "@/components/ui";
import { cn } from "@/lib/cn";
import { useDocumentTitle, useOnChange } from "@/lib/hooks";
import { ProxyAddresses } from "./ProxyAddresses";

type Mode = "local" | "networks" | "all";

const MODES: { value: Mode; label: string; hint: string }[] = [
  {
    value: "local",
    label: "Only this computer",
    hint: "For a reverse proxy on this computer that serves Pley with HTTPS (Caddy, nginx, IIS), or for Pley on its own.",
  },
  {
    value: "networks",
    label: "This computer and the networks chosen",
    hint: "Devices on those networks reach Pley at this computer's address there.",
  },
  { value: "all", label: "Every network", hint: "Those that come later too: another Wi-Fi, a VPN." },
];

const NETWORK_KIND: Record<NonNullable<NetworkAdapter["network"]>["kind"], string> = {
  private: "Private network",
  domain: "Domain network",
  public: "Public network",
};

const LISTENER: Record<ListenerKind, string> = {
  web: "Pley",
  jellyfin: "Jellyfin-compatible API",
  discovery: "Discovery (UDP)",
};

const DEFAULT_PORT = 7539;
/** Jellyfin's own, which its apps try first. */
const JELLYFIN_PORT = 8096;

/** What the page edits; {@link choiceOf} turns it into addresses. */
interface Form {
  mode: Mode;
  /** The networks' addresses, for "networks". */
  ips: string[];
  port: number;
  jellyfin: boolean;
  /** On pley's port rather than one of its own. */
  jellyfinOnWeb: boolean;
  jellyfinPort: number;
  discovery: boolean;
  bonjour: boolean;
  /** The adapters Bonjour announces from: every one when empty. */
  bonjourAdapters: string[];
}

/** `192.168.1.10:7539` or `[::1]:7539`: its address and port. */
export function splitAddress(address: string): { ip: string; port: number } {
  const at = address.lastIndexOf(":");
  return { ip: address.slice(0, at).replace(/^\[(.*)\]$/, "$1"), port: Number(address.slice(at + 1)) };
}

const joinAddress = (ip: string, port: number) => (ip.includes(":") ? `[${ip}]:${port}` : `${ip}:${port}`);
const isLoopback = (ip: string) => ip.startsWith("127.") || ip === "::1";
const isAny = (ip: string) => ip === "0.0.0.0" || ip === "::";

function formOf(choice: ListenChoice): Form {
  const web = choice.web.map(splitAddress);
  const jellyfin = choice.jellyfin.map(splitAddress);
  const mode: Mode = web.some((a) => isAny(a.ip)) ? "all" : web.every((a) => isLoopback(a.ip)) ? "local" : "networks";
  return {
    mode,
    ips: web.filter((a) => !isLoopback(a.ip) && !isAny(a.ip)).map((a) => a.ip),
    port: web[0]?.port ?? DEFAULT_PORT,
    jellyfin: jellyfin.length > 0 || choice.jellyfin_on_web,
    jellyfinOnWeb: jellyfin.length === 0,
    jellyfinPort: jellyfin[0]?.port ?? JELLYFIN_PORT,
    discovery: choice.discovery,
    bonjour: choice.bonjour,
    bonjourAdapters: choice.bonjour_adapters,
  };
}

/** The addresses `form` stands for, but what the command line or the environment sets. */
function choiceOf(form: Form, network: Network): ListenChoice {
  const ips = form.mode === "all" ? ["0.0.0.0"] : form.mode === "local" ? ["127.0.0.1"] : ["127.0.0.1", ...form.ips];
  return {
    web: network.fixed.web ? network.web : ips.map((ip) => joinAddress(ip, form.port)),
    jellyfin: network.fixed.jellyfin
      ? network.jellyfin
      : form.jellyfin && !form.jellyfinOnWeb
        ? ips.map((ip) => joinAddress(ip, form.jellyfinPort))
        : [],
    jellyfin_on_web: network.fixed.jellyfin ? network.jellyfin_on_web : form.jellyfin && form.jellyfinOnWeb,
    discovery: network.fixed.discovery ? network.discovery : form.discovery,
    bonjour: network.fixed.bonjour ? network.bonjour : form.bonjour,
    bonjour_adapters: [...form.bonjourAdapters].sort(),
  };
}

const sameChoice = (a: ListenChoice, b: ListenChoice) =>
  a.discovery === b.discovery &&
  a.bonjour === b.bonjour &&
  a.bonjour_adapters.join("\n") === b.bonjour_adapters.join("\n") &&
  a.jellyfin_on_web === b.jellyfin_on_web &&
  a.web.join() === b.web.join() &&
  a.jellyfin.join() === b.jellyfin.join();

/** Why `form` can't be saved, if it can't. */
function problemOf(form: Form): string | null {
  const port = (p: number) => Number.isInteger(p) && p >= 1 && p <= 65535;
  if (!port(form.port)) return "The port is a number from 1 to 65535.";
  if (form.jellyfin && !form.jellyfinOnWeb) {
    if (!port(form.jellyfinPort)) return "The Jellyfin-compatible API's port is a number from 1 to 65535.";
    if (form.jellyfinPort === form.port) return "Choose another port for the Jellyfin-compatible API, or Pley's port.";
  }
  if (form.mode === "networks" && form.ips.length === 0) return "Choose a network, or Only this computer.";
  return null;
}


/** This page's host as pley's addresses write it, or `null` for a name (a reverse proxy's, say), which says nothing. */
function pageHost(url: URL): string | null {
  const host = url.hostname.replace(/^\[(.*)\]$/, "$1");
  if (host === "localhost") return "127.0.0.1";
  return /^\d{1,3}(\.\d{1,3}){3}$/.test(host) || host.includes(":") ? host : null;
}

const pagePort = (url: URL) => Number(url.port || (url.protocol === "https:" ? 443 : 80));
const reaches = (ip: string, host: string) => isAny(ip) || ip === host || (isLoopback(host) && isLoopback(ip));

/**
 * Whether pley, listening on `next` instead of `now`, still serves the page
 * at `href`: `null` when it doesn't serve it directly now (a reverse proxy
 * does), so can't tell.
 */
export function stillServes(now: string[], next: string[], href: string): boolean | null {
  const url = new URL(href);
  const host = pageHost(url);
  if (!host) return null;
  const port = pagePort(url);
  const serves = (list: string[]) => list.map(splitAddress).some((a) => a.port === port && reaches(a.ip, host));
  return serves(now) ? serves(next) : null;
}

/** The page at `href` where pley serves it once it listens on `next` (another port), or `null`. */
export function movedPage(now: string[], next: string[], href: string): string | null {
  if (stillServes(now, next, href) !== false) return null;
  const url = new URL(href);
  const host = pageHost(url) ?? "";
  const there = next.map(splitAddress).find((a) => reaches(a.ip, host));
  if (!there) return null;
  url.port = String(there.port);
  return url.toString();
}

function Row({ label, hint, children, htmlFor }: { label: string; hint?: ReactNode; children: ReactNode; htmlFor?: string }) {
  return (
    <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)] sm:items-center sm:gap-6">
      <div>
        <label htmlFor={htmlFor} className="text-sm font-medium">
          {label}
        </label>
        {hint ? <div className="mt-0.5 text-xs text-subtle">{hint}</div> : null}
      </div>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

function Note({ icon, tone = "muted", children }: { icon: ReactNode; tone?: "muted" | "warning"; children: ReactNode }) {
  return (
    <p className={cn("flex items-start gap-2 text-xs", tone === "warning" ? "text-danger" : "text-muted")}>
      <span className="mt-px shrink-0">{icon}</span>
      <span>{children}</span>
    </p>
  );
}

/** A checkbox row, as the libraries' in AccessFields. */
function Choice({ id, checked, disabled, onChange, children }: { id: string; checked: boolean; disabled?: boolean; onChange: () => void; children: ReactNode }) {
  return (
    <label
      htmlFor={id}
      className={cn("flex items-start gap-3 rounded px-2 py-2 text-sm", disabled ? "opacity-60" : "cursor-pointer hover:bg-white/5")}
    >
      <input id={id} type="checkbox" checked={checked} disabled={disabled} onChange={onChange} className="peer sr-only" />
      <span
        className={cn(
          "mt-0.5 flex size-4 shrink-0 items-center justify-center rounded border transition-colors peer-focus-visible:ring-2 peer-focus-visible:ring-accent",
          checked ? "border-accent bg-accent text-accent-fg" : "border-border-strong",
        )}
      >
        {checked ? <Check className="size-3" strokeWidth={3} /> : null}
      </span>
      <span className="min-w-0 flex-1">{children}</span>
    </label>
  );
}

function Adapters({
  adapters,
  ips,
  disabled,
  onChange,
}: {
  adapters: NetworkAdapter[];
  ips: string[];
  disabled: boolean;
  onChange: (ips: string[]) => void;
}) {
  const known = new Set(adapters.flatMap((a) => a.addresses));
  // Chosen before, on an adapter that isn't here now (a VPN that is off).
  const elsewhere = ips.filter((ip) => !known.has(ip));
  return (
    <div role="group" aria-label="Networks" className="space-y-0.5 rounded-md border border-border p-1.5">
      {adapters.map((adapter, i) => {
        const checked = adapter.addresses.some((ip) => ips.includes(ip));
        const toggle = () =>
          onChange(checked ? ips.filter((ip) => !adapter.addresses.includes(ip)) : [...ips, ...adapter.addresses.filter((ip) => !ips.includes(ip))]);
        return (
          <Choice key={`${i} ${adapter.name}`} id={`network-adapter-${i}`} checked={checked} disabled={disabled} onChange={toggle}>
            <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
              <span className="font-medium">{adapter.name}</span>
              {adapter.network ? (
                <Badge tone={adapter.network.kind === "public" ? "danger" : "info"}>{NETWORK_KIND[adapter.network.kind]}</Badge>
              ) : null}
              {adapter.up ? null : <Badge>Not connected</Badge>}
            </span>
            <span className="block truncate text-xs text-subtle">
              {[adapter.addresses.join(", "), adapter.network?.name, adapter.description].filter(Boolean).join(" · ")}
            </span>
          </Choice>
        );
      })}
      {elsewhere.map((ip) => (
        <Choice key={ip} id={`network-elsewhere-${ip}`} checked disabled={disabled} onChange={() => onChange(ips.filter((x) => x !== ip))}>
          <span className="font-mono text-xs">{ip}</span>
          <span className="block text-xs text-subtle">Not on this computer now: Pley listens there once it is.</span>
        </Choice>
      ))}
      {adapters.length === 0 && elsewhere.length === 0 ? <p className="px-2 py-1.5 text-sm text-subtle">No network adapters found.</p> : null}
    </div>
  );
}

/** The adapters Bonjour announces from, by name; "Every network" when none is chosen. */
function BonjourAdapters({
  adapters,
  chosen,
  disabled,
  onChange,
}: {
  adapters: NetworkAdapter[];
  chosen: string[];
  disabled: boolean;
  onChange: (chosen: string[]) => void;
}) {
  const names = [...new Set(adapters.map((a) => a.name))];
  // Chosen before, on an adapter that isn't here now (a VPN that is off), or
  // one Pley no longer listens on.
  const elsewhere = chosen.filter((name) => !names.includes(name));
  return (
    <div role="group" aria-label="Announced on" className="space-y-0.5 rounded-md border border-border p-1.5">
      <Choice id="bonjour-every-adapter" checked={chosen.length === 0} disabled={disabled} onChange={() => onChange([])}>
        <span className="font-medium">Every network Pley listens on</span>
        <span className="block text-xs text-subtle">Those that come later too.</span>
      </Choice>
      {names.map((name, i) => {
        const adapter = adapters.find((a) => a.name === name);
        const checked = chosen.includes(name);
        return (
          <Choice
            key={`${i} ${name}`}
            id={`bonjour-adapter-${i}`}
            checked={checked}
            disabled={disabled}
            onChange={() => onChange(checked ? chosen.filter((n) => n !== name) : [...chosen, name])}
          >
            <span className="font-medium">{name}</span>
            <span className="block truncate text-xs text-subtle">
              {[adapter?.addresses.join(", "), adapter?.network?.name].filter(Boolean).join(" · ")}
            </span>
          </Choice>
        );
      })}
      {elsewhere.map((name, i) => (
        <Choice key={name} id={`bonjour-elsewhere-${i}`} checked disabled={disabled} onChange={() => onChange(chosen.filter((n) => n !== name))}>
          <span className="font-medium">{name}</span>
          <span className="block text-xs text-subtle">Not a network Pley listens on now: announced there once it is.</span>
        </Choice>
      ))}
    </div>
  );
}

export function NetworkSettings() {
  useDocumentTitle("Network settings");
  const network = useNetwork();
  const save = useSaveNetwork();
  const toast = useToast();
  const data = network.data;
  // The form starts from where pley listens, and again when that changes
  // (not when only how its listeners are does).
  const [form, setForm] = useState<Form | null>(() => (data ? formOf(data) : null));
  const shown = data
    ? JSON.stringify([data.web, data.jellyfin, data.jellyfin_on_web, data.discovery, data.bonjour, data.bonjour_adapters])
    : "";
  useOnChange(shown, () => {
    if (data) setForm(formOf(data));
  });

  if (network.isPending || (!form && !network.isError)) return <PageSpinner />;
  if (network.isError || !form || !data) {
    return (
      <EmptyState
        title="Couldn't load the network settings"
        description={errorMessage(network.error)}
        action={<Button onClick={() => void network.refetch()}>Retry</Button>}
      />
    );
  }

  const set = (patch: Partial<Form>) => setForm((f) => (f ? { ...f, ...patch } : f));
  const saved = choiceOf(formOf(data), data);
  const next = choiceOf(form, data);
  const dirty = !sameChoice(next, saved);
  // Changed on the page, if not yet in its addresses ("networks", none chosen).
  const edited = JSON.stringify(form) !== JSON.stringify(formOf(data));
  const problem = problemOf(form);
  const href = window.location.href;
  const moved = dirty ? movedPage(data.web, next.web, href) : null;
  const lost = dirty && !moved && stillServes(data.web, next.web, href) === false;
  // pley.toml lists addresses this page doesn't show as they are.
  const unusual = !sameChoice(saved, data);
  const fixedAll = data.fixed.web && data.fixed.jellyfin && data.fixed.discovery && data.fixed.bonjour;

  const onSave = () => {
    if (problem) return;
    save.mutate(next, {
      onSuccess: (after) => {
        const waiting = after.listeners.some((l) => l.error);
        toast.success(waiting ? "Saved: Pley listens on the rest once it can" : "Pley listens there now");
        const page = movedPage(data.web, after.web, window.location.href);
        if (page) window.location.assign(page);
      },
      onError: (e) => toast.error(errorMessage(e)),
    });
  };

  return (
    <form
      className="space-y-5 pb-20"
      onSubmit={(e) => {
        e.preventDefault();
        onSave();
      }}
    >
      <div>
        <h2 className="text-lg font-semibold">Network</h2>
        <p className="text-sm text-muted">
          Where Pley listens for browsers and apps. Changes apply at once, and Pley keeps them in{" "}
          <code className="break-all font-mono text-xs">{data.config_file}</code>.
        </p>
      </div>

      <Card className="space-y-4">
        <div className="space-y-1.5">
          <h3 className="text-base font-semibold">Listen on</h3>
          {data.fixed.web ? (
            <Note icon={<Lock className="size-3.5" />}>
              Set by <code className="font-mono">--bind</code> or <code className="font-mono">PLEY_BIND</code>, on the command line or in the
              environment: change it there.
            </Note>
          ) : null}
        </div>
        <div className="grid gap-2" role="radiogroup" aria-label="Listen on">
          {MODES.map(({ value, label, hint }) => (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={form.mode === value}
              disabled={data.fixed.web}
              onClick={() => set({ mode: value })}
              className={cn(
                "rounded-md border px-3 py-2.5 text-left transition-colors disabled:opacity-60",
                form.mode === value ? "border-accent bg-accent/10" : "border-border hover:border-border-strong",
              )}
            >
              <span className="block text-sm font-medium text-fg">{label}</span>
              <span className="block text-xs text-muted">{hint}</span>
            </button>
          ))}
        </div>
        {form.mode === "networks" ? (
          <Adapters adapters={data.adapters} ips={form.ips} disabled={data.fixed.web} onChange={(ips) => set({ ips })} />
        ) : null}
        {data.windows && form.mode !== "local" ? (
          <Note icon={<Info className="size-3.5" />}>
            Windows Firewall decides which kinds of network reach Pley: Pley's Setup lets it through on private and domain networks, unless you
            chose otherwise there (run Setup again to change that).
          </Note>
        ) : null}
        <Row label="Port" htmlFor="network-port" hint="The web app's and the Pley apps'.">
          <TextInput
            id="network-port"
            type="number"
            min={1}
            max={65535}
            value={Number.isNaN(form.port) ? "" : form.port}
            disabled={data.fixed.web}
            onChange={(e) => set({ port: Number.parseInt(e.target.value, 10) })}
            className="w-28"
          />
        </Row>
      </Card>

      <Card className="space-y-5">
        <Row
          label="Jellyfin-compatible API"
          hint={
            data.fixed.jellyfin
              ? "Set by --jellyfin-bind or PLEY_JELLYFIN_BIND, on the command line or in the environment."
              : !form.jellyfin
                ? "For Jellyfin's apps, Infuse and other apps that use Jellyfin's API. They sign in with Pley accounts."
                : form.jellyfinOnWeb
                  ? "On Pley's address, through your reverse proxy too: apps connect to the address you use for Pley."
                  : "On a port of its own, at the addresses above."
          }
        >
          <Toggle
            checked={form.jellyfin}
            disabled={data.fixed.jellyfin}
            onChange={(v) => set({ jellyfin: v })}
            label="Jellyfin-compatible API"
          />
        </Row>
        {form.jellyfin ? (
          <div className="grid grid-cols-2 gap-1 rounded-md bg-surface-2 p-1 text-sm" role="radiogroup" aria-label="Jellyfin-compatible API's port">
            {[
              { onWeb: true, label: `On Pley's port (${Number.isNaN(form.port) ? DEFAULT_PORT : form.port})` },
              { onWeb: false, label: "On a port of its own" },
            ].map((o) => (
              <button
                key={o.label}
                type="button"
                role="radio"
                aria-checked={form.jellyfinOnWeb === o.onWeb}
                disabled={data.fixed.jellyfin}
                onClick={() => set({ jellyfinOnWeb: o.onWeb })}
                className={cn(
                  "h-8 rounded font-medium transition-colors disabled:opacity-60",
                  form.jellyfinOnWeb === o.onWeb ? "bg-surface-3 text-fg shadow-sm" : "text-muted hover:text-fg",
                )}
              >
                {o.label}
              </button>
            ))}
          </div>
        ) : null}
        {form.jellyfin && !form.jellyfinOnWeb ? (
          <Row label="Jellyfin port" htmlFor="network-jellyfin-port" hint={`Jellyfin's own is ${JELLYFIN_PORT}.`}>
            <TextInput
              id="network-jellyfin-port"
              type="number"
              min={1}
              max={65535}
              value={Number.isNaN(form.jellyfinPort) ? "" : form.jellyfinPort}
              disabled={data.fixed.jellyfin}
              onChange={(e) => set({ jellyfinPort: Number.parseInt(e.target.value, 10) })}
              className="w-28"
            />
          </Row>
        ) : null}
        <Row
          label="Found by the Pley apps"
          hint={
            data.fixed.discovery
              ? "Turned off by --no-discovery or PLEY_NO_DISCOVERY."
              : form.mode === "local"
                ? "On networks only: devices can't reach this computer's own address."
                : "The Pley apps on these networks find this server by themselves."
          }
        >
          <Toggle
            checked={form.discovery && form.mode !== "local"}
            disabled={data.fixed.discovery || form.mode === "local"}
            onChange={(v) => set({ discovery: v })}
            label="Found by the Pley apps"
          />
        </Row>
      </Card>

      <Card className="space-y-4">
        <Row
          label="Bonjour"
          hint={
            data.fixed.bonjour
              ? "Turned off by --no-bonjour, --no-discovery, PLEY_NO_BONJOUR or PLEY_NO_DISCOVERY."
              : form.mode === "local"
                ? "On networks only: devices can't reach this computer's own address."
                : "Apple's devices find this server by Bonjour."
          }
        >
          <Toggle
            checked={form.bonjour && form.mode !== "local"}
            disabled={data.fixed.bonjour || form.mode === "local"}
            onChange={(v) => set({ bonjour: v })}
            label="Bonjour"
          />
        </Row>
        {form.bonjour && form.mode !== "local" ? (
            <div className="space-y-1.5">
              <h4 className="text-sm font-medium">Announced on</h4>
              <p className="text-xs text-subtle">
                With a VPN, a virtual machine's or container's network, or a guest Wi-Fi, choose the networks whose devices should find Pley. Each
                network is told only Pley's address on it.
              </p>
              <BonjourAdapters
                // Only where Pley listens: announced elsewhere, it couldn't be reached.
                adapters={form.mode === "networks" ? data.adapters.filter((a) => a.addresses.some((ip) => form.ips.includes(ip))) : data.adapters}
                chosen={form.bonjourAdapters}
                disabled={data.fixed.bonjour}
                onChange={(bonjourAdapters) => set({ bonjourAdapters })}
              />
            </div>
        ) : null}
      </Card>

      <ProxyAddresses />

      <Card className="space-y-3">
        <div>
          <h3 className="text-base font-semibold">Listening now</h3>
          <p className="mt-0.5 text-sm text-muted">Pley retries addresses it can't listen on yet every 30 seconds.</p>
        </div>
        <ul className="space-y-1.5 text-sm">
          {data.jellyfin_on_web ? (
            <li className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
              <CircleCheck className="size-3.5 shrink-0 text-success" />
              <span className="text-muted">{LISTENER.jellyfin}, on Pley's port</span>
            </li>
          ) : null}
          {data.listeners.map((l) => (
            <li key={`${l.kind} ${l.address}`} className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
              {l.error ? <CircleAlert className="size-3.5 shrink-0 text-danger" /> : <CircleCheck className="size-3.5 shrink-0 text-success" />}
              <span className="font-mono text-xs">{l.address}</span>
              <span className="text-muted">{LISTENER[l.kind]}</span>
              {l.error ? <span className="basis-full pl-5.5 text-xs text-danger">Can't listen: {l.error}</span> : null}
            </li>
          ))}
        </ul>
      </Card>

      {unusual && !fixedAll ? (
        <Note icon={<Info className="size-3.5" />}>
          Pley listens on {[...data.web, ...data.jellyfin].join(", ")}, which this page shows as near as it can: saving replaces them.
        </Note>
      ) : null}

      {/* Floats over the page only with something to save: at rest it would cover the rows on a phone. */}
      <div className={cn("flex justify-end", (edited || save.isPending) && "sticky bottom-4 z-10")}>
        <div className="flex max-w-full flex-wrap items-center justify-end gap-3 rounded-lg border border-border bg-surface-2/95 px-3 py-2 shadow-xl backdrop-blur">
          {edited && problem ? (
            <span className="text-xs text-danger">{problem}</span>
          ) : lost ? (
            <span className="text-xs text-danger">Pley would stop listening where this page is: it would stop working here.</span>
          ) : moved ? (
            <span className="text-xs text-muted">This page moves to the new port.</span>
          ) : dirty ? (
            <span className="text-xs text-muted">Unsaved changes</span>
          ) : null}
          <Button variant="ghost" size="sm" disabled={!edited || save.isPending} onClick={() => setForm(formOf(data))}>
            Reset
          </Button>
          <Button
            type="submit"
            variant="primary"
            size="sm"
            icon={<Save className="size-4" />}
            loading={save.isPending}
            disabled={!dirty || problem !== null}
          >
            Save
          </Button>
        </div>
      </div>
    </form>
  );
}
