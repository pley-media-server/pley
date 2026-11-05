import { useState, type ReactNode } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Check, CircleAlert, CircleCheck, Info, Lock, Save } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useNetwork, useSaveNetwork } from "@/api/hooks";
import type { ListenChoice, ListenerKind, NetworkAdapter, NetworkSettings as Network } from "@/api/types";
import { useToast } from "@/components/Toast";
import { Badge, Button, Card, EmptyState, PageSpinner, TextInput, Toggle } from "@/components/ui";
import { t as translate } from "@/i18n";
import { cn } from "@/lib/cn";
import { useDocumentTitle, useOnChange } from "@/lib/hooks";
import { ProxyAddresses } from "./ProxyAddresses";

type Mode = "local" | "networks" | "all";

const MODES: Mode[] = ["local", "networks", "all"];

/** What a listener is, as the page says it. */
const listenerName = (kind: ListenerKind) => (kind === "web" ? "Pley" : translate(`network.listeners.${kind}`, { ns: "settings" }));

const DEFAULT_PORT = 7539;
/** Jellyfin's own, which its apps try first. */
const JELLYFIN_PORT = 8096;
/** Plex's own, which its tools expect. */
const PLEX_PORT = 32400;

type Api = "jellyfin" | "plex";

/** The APIs for other apps, in the page's order, with their own ports. */
const APIS: { api: Api; port: number }[] = [
  { api: "jellyfin", port: JELLYFIN_PORT },
  { api: "plex", port: PLEX_PORT },
];

/** Whether an API for other apps is on, and where. */
interface ApiForm {
  on: boolean;
  /** On pley's port rather than one of its own. */
  onWeb: boolean;
  port: number;
}

/** What the page edits; {@link choiceOf} turns it into addresses. */
interface Form {
  mode: Mode;
  /** The networks' addresses, for "networks". */
  ips: string[];
  port: number;
  jellyfin: ApiForm;
  plex: ApiForm;
  discovery: boolean;
  bonjour: boolean;
  /** The adapters Bonjour announces from: every one when empty. */
  bonjourAdapters: string[];
  /** Bonjour's host name as typed: the generated one when empty. */
  bonjourHost: string;
}

/** A host name as typed (`Media`, `media.local`) as the server keeps it: its label, lowercase. */
export const hostLabel = (typed: string) =>
  typed.trim().replace(/\.+$/, "").toLowerCase().replace(/\.local$/, "");

/** One DNS label of letters, digits and hyphens, not starting or ending with one; empty for the generated name. */
const HOST_LABEL = /^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)?$/;

/** `192.168.1.10:7539` or `[::1]:7539`: its address and port. */
export function splitAddress(address: string): { ip: string; port: number } {
  const at = address.lastIndexOf(":");
  return { ip: address.slice(0, at).replace(/^\[(.*)\]$/, "$1"), port: Number(address.slice(at + 1)) };
}

const joinAddress = (ip: string, port: number) => (ip.includes(":") ? `[${ip}]:${port}` : `${ip}:${port}`);
const isLoopback = (ip: string) => ip.startsWith("127.") || ip === "::1";
const isAny = (ip: string) => ip === "0.0.0.0" || ip === "::";

/** An API's form from its addresses of its own and whether it answers on pley's port. */
function apiFormOf(addresses: string[], onWeb: boolean, port: number): ApiForm {
  const own = addresses.map(splitAddress);
  return { on: own.length > 0 || onWeb, onWeb: own.length === 0, port: own[0]?.port ?? port };
}

function formOf(choice: ListenChoice): Form {
  const web = choice.web.map(splitAddress);
  const mode: Mode = web.some((a) => isAny(a.ip)) ? "all" : web.every((a) => isLoopback(a.ip)) ? "local" : "networks";
  return {
    mode,
    ips: web.filter((a) => !isLoopback(a.ip) && !isAny(a.ip)).map((a) => a.ip),
    port: web[0]?.port ?? DEFAULT_PORT,
    jellyfin: apiFormOf(choice.jellyfin, choice.jellyfin_on_web, JELLYFIN_PORT),
    plex: apiFormOf(choice.plex, choice.plex_on_web, PLEX_PORT),
    discovery: choice.discovery,
    bonjour: choice.bonjour,
    bonjourAdapters: choice.bonjour_adapters,
    bonjourHost: choice.bonjour_host_name,
  };
}

/** The addresses `form` stands for, but what the command line or the environment sets. */
function choiceOf(form: Form, network: Network): ListenChoice {
  const ips = form.mode === "all" ? ["0.0.0.0"] : form.mode === "local" ? ["127.0.0.1"] : ["127.0.0.1", ...form.ips];
  const own = (api: ApiForm) => (api.on && !api.onWeb ? ips.map((ip) => joinAddress(ip, api.port)) : []);
  // Both APIs, always: the server takes one left out for off.
  return {
    web: network.fixed.web ? network.web : ips.map((ip) => joinAddress(ip, form.port)),
    jellyfin: network.fixed.jellyfin ? network.jellyfin : own(form.jellyfin),
    jellyfin_on_web: network.fixed.jellyfin ? network.jellyfin_on_web : form.jellyfin.on && form.jellyfin.onWeb,
    plex: network.fixed.plex ? network.plex : own(form.plex),
    plex_on_web: network.fixed.plex ? network.plex_on_web : form.plex.on && form.plex.onWeb,
    discovery: network.fixed.discovery ? network.discovery : form.discovery,
    bonjour: network.fixed.bonjour ? network.bonjour : form.bonjour,
    bonjour_adapters: [...form.bonjourAdapters].sort(),
    bonjour_host_name: HOST_LABEL.test(hostLabel(form.bonjourHost)) ? hostLabel(form.bonjourHost) : network.bonjour_host_name,
  };
}

const sameChoice = (a: ListenChoice, b: ListenChoice) =>
  a.discovery === b.discovery &&
  a.bonjour === b.bonjour &&
  a.bonjour_adapters.join("\n") === b.bonjour_adapters.join("\n") &&
  a.bonjour_host_name === b.bonjour_host_name &&
  a.jellyfin_on_web === b.jellyfin_on_web &&
  a.plex_on_web === b.plex_on_web &&
  a.web.join() === b.web.join() &&
  a.jellyfin.join() === b.jellyfin.join() &&
  a.plex.join() === b.plex.join();

/** Why `form` can't be saved, if it can't. */
function problemOf(form: Form): string | null {
  const port = (p: number) => Number.isInteger(p) && p >= 1 && p <= 65535;
  if (!port(form.port)) return translate("network.problems.port", { ns: "settings" });
  for (const { api } of APIS) {
    const { on, onWeb, port: own } = form[api];
    if (!on || onWeb) continue;
    if (!port(own)) return translate(`network.apis.${api}.portRange`, { ns: "settings" });
    if (own === form.port) return translate(`network.apis.${api}.samePort`, { ns: "settings" });
  }
  const { jellyfin, plex } = form;
  if (jellyfin.on && !jellyfin.onWeb && plex.on && !plex.onWeb && jellyfin.port === plex.port) {
    return translate("network.problems.sharedPort", { ns: "settings" });
  }
  if (form.mode === "networks" && form.ips.length === 0) return translate("network.problems.noNetwork", { ns: "settings" });
  // Only where it shows: hidden, the saved one stays (see choiceOf).
  if (form.bonjour && form.mode !== "local" && !HOST_LABEL.test(hostLabel(form.bonjourHost))) {
    return translate("network.problems.hostName", { ns: "settings" });
  }
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
  const { t } = useTranslation("settings");
  const known = new Set(adapters.flatMap((a) => a.addresses));
  // Chosen before, on an adapter that isn't here now (a VPN that is off).
  const elsewhere = ips.filter((ip) => !known.has(ip));
  return (
    <div role="group" aria-label={t("network.adapters.label")} className="space-y-0.5 rounded-md border border-border p-1.5">
      {adapters.map((adapter, i) => {
        const checked = adapter.addresses.some((ip) => ips.includes(ip));
        const toggle = () =>
          onChange(checked ? ips.filter((ip) => !adapter.addresses.includes(ip)) : [...ips, ...adapter.addresses.filter((ip) => !ips.includes(ip))]);
        return (
          <Choice key={`${i} ${adapter.name}`} id={`network-adapter-${i}`} checked={checked} disabled={disabled} onChange={toggle}>
            <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
              <span className="font-medium">{adapter.name}</span>
              {adapter.network ? (
                <Badge tone={adapter.network.kind === "public" ? "danger" : "info"}>{t(`network.kinds.${adapter.network.kind}`)}</Badge>
              ) : null}
              {adapter.up ? null : <Badge>{t("network.adapters.notConnected")}</Badge>}
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
          <span className="block text-xs text-subtle">{t("network.adapters.elsewhere")}</span>
        </Choice>
      ))}
      {adapters.length === 0 && elsewhere.length === 0 ? <p className="px-2 py-1.5 text-sm text-subtle">{t("network.adapters.none")}</p> : null}
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
  const { t } = useTranslation("settings");
  const names = [...new Set(adapters.map((a) => a.name))];
  // Chosen before, on an adapter that isn't here now (a VPN that is off), or
  // one Pley no longer listens on.
  const elsewhere = chosen.filter((name) => !names.includes(name));
  return (
    <div role="group" aria-label={t("network.bonjour.announcedOn")} className="space-y-0.5 rounded-md border border-border p-1.5">
      <Choice id="bonjour-every-adapter" checked={chosen.length === 0} disabled={disabled} onChange={() => onChange([])}>
        <span className="font-medium">{t("network.bonjour.every")}</span>
        <span className="block text-xs text-subtle">{t("network.bonjour.everyHint")}</span>
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
          <span className="block text-xs text-subtle">{t("network.bonjour.elsewhere")}</span>
        </Choice>
      ))}
    </div>
  );
}

/** An API for other apps: off, on pley's port, or on a port of its own. */
function ApiChoice({
  api,
  value,
  fixed,
  webPort,
  onChange,
}: {
  api: (typeof APIS)[number];
  value: ApiForm;
  fixed: boolean;
  webPort: number;
  onChange: (value: ApiForm) => void;
}) {
  const { t } = useTranslation("settings");
  const set = (patch: Partial<ApiForm>) => onChange({ ...value, ...patch });
  const portId = `network-${api.api}-port`;
  const name = listenerName(api.api);
  return (
    <>
      <Row label={name} hint={fixed ? t(`network.apis.${api.api}.fixed`) : t(`network.apis.${api.api}.hint`)}>
        <Toggle checked={value.on} disabled={fixed} onChange={(v) => set({ on: v })} label={name} />
      </Row>
      {value.on ? (
        <div className="grid grid-cols-2 gap-1 rounded-md bg-surface-2 p-1 text-sm" role="radiogroup" aria-label={t(`network.apis.${api.api}.portChoice`)}>
          {[
            { onWeb: true, label: t("network.onPleyPort", { port: Number.isNaN(webPort) ? DEFAULT_PORT : webPort }) },
            { onWeb: false, label: t("network.ownPort") },
          ].map((o) => (
            <button
              key={o.label}
              type="button"
              role="radio"
              aria-checked={value.onWeb === o.onWeb}
              disabled={fixed}
              onClick={() => set({ onWeb: o.onWeb })}
              className={cn(
                "h-8 rounded font-medium transition-colors disabled:opacity-60",
                value.onWeb === o.onWeb ? "bg-surface-3 text-fg shadow-sm" : "text-muted hover:text-fg",
              )}
            >
              {o.label}
            </button>
          ))}
        </div>
      ) : null}
      {value.on && !value.onWeb ? (
        <Row label={t(`network.apis.${api.api}.portLabel`)} htmlFor={portId} hint={t(`network.apis.${api.api}.portHint`, { port: api.port })}>
          <TextInput
            id={portId}
            type="number"
            min={1}
            max={65535}
            value={Number.isNaN(value.port) ? "" : value.port}
            disabled={fixed}
            onChange={(e) => set({ port: Number.parseInt(e.target.value, 10) })}
            className="w-28"
          />
        </Row>
      ) : null}
    </>
  );
}

export function NetworkSettings() {
  const { t } = useTranslation(["settings", "common"]);
  useDocumentTitle(t("network.documentTitle"));
  const network = useNetwork();
  const save = useSaveNetwork();
  const toast = useToast();
  const data = network.data;
  // The form starts from where pley listens, and again when that changes
  // (not when only how its listeners are does).
  const [form, setForm] = useState<Form | null>(() => (data ? formOf(data) : null));
  const where = data && [
    data.web,
    data.jellyfin,
    data.jellyfin_on_web,
    data.plex,
    data.plex_on_web,
    data.discovery,
    data.bonjour,
    data.bonjour_adapters,
    data.bonjour_host_name,
  ];
  useOnChange(where ? JSON.stringify(where) : "", () => {
    if (data) setForm(formOf(data));
  });

  if (network.isPending || (!form && !network.isError)) return <PageSpinner />;
  if (network.isError || !form || !data) {
    return (
      <EmptyState
        title={t("network.couldntLoad")}
        description={errorMessage(network.error)}
        action={<Button onClick={() => void network.refetch()}>{t("common:actions.retry")}</Button>}
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
  const fixedAll =
    data.fixed.web && data.fixed.jellyfin && data.fixed.plex && data.fixed.discovery && data.fixed.bonjour;

  const onSave = () => {
    if (problem) return;
    save.mutate(next, {
      onSuccess: (after) => {
        const waiting = after.listeners.some((l) => l.error);
        toast.success(waiting ? t("network.savedWaiting") : t("network.listensNow"));
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
        <h2 className="text-lg font-semibold">{t("network.title")}</h2>
        <p className="text-sm text-muted">
          <Trans
            t={t}
            i18nKey="network.description"
            values={{ file: data.config_file }}
            components={{ file: <code className="break-all font-mono text-xs" /> }}
          />
        </p>
      </div>

      <Card className="space-y-4">
        <div className="space-y-1.5">
          <h3 className="text-base font-semibold">{t("network.listenOn")}</h3>
          {data.fixed.web ? (
            <Note icon={<Lock className="size-3.5" />}>
              <Trans t={t} i18nKey="network.fixedBind" components={{ code: <code className="font-mono" /> }} />
            </Note>
          ) : null}
        </div>
        <div className="grid gap-2" role="radiogroup" aria-label={t("network.listenOn")}>
          {MODES.map((value) => (
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
              <span className="block text-sm font-medium text-fg">{t(`network.modes.${value}.label`)}</span>
              <span className="block text-xs text-muted">{t(`network.modes.${value}.hint`)}</span>
            </button>
          ))}
        </div>
        {form.mode === "networks" ? (
          <Adapters adapters={data.adapters} ips={form.ips} disabled={data.fixed.web} onChange={(ips) => set({ ips })} />
        ) : null}
        {data.windows && form.mode !== "local" ? (
          <Note icon={<Info className="size-3.5" />}>{t("network.firewall")}</Note>
        ) : null}
        <Row label={t("network.port")} htmlFor="network-port" hint={t("network.portHint")}>
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
        {APIS.map((api) => (
          <ApiChoice
            key={api.api}
            api={api}
            value={form[api.api]}
            fixed={data.fixed[api.api]}
            webPort={form.port}
            onChange={(value) => set({ [api.api]: value })}
          />
        ))}
        <Row
          label={t("network.discovery.label")}
          hint={
            data.fixed.discovery
              ? t("network.discovery.fixed")
              : form.mode === "local"
                ? t("network.localOnly")
                : t("network.discovery.hint")
          }
        >
          <Toggle
            checked={form.discovery && form.mode !== "local"}
            disabled={data.fixed.discovery || form.mode === "local"}
            onChange={(v) => set({ discovery: v })}
            label={t("network.discovery.label")}
          />
        </Row>
      </Card>

      <Card className="space-y-4">
        <Row
          label={t("network.bonjour.label")}
          hint={
            data.fixed.bonjour
              ? t("network.bonjour.fixed")
              : form.mode === "local"
                ? t("network.localOnly")
                : t("network.bonjour.hint")
          }
        >
          <Toggle
            checked={form.bonjour && form.mode !== "local"}
            disabled={data.fixed.bonjour || form.mode === "local"}
            onChange={(v) => set({ bonjour: v })}
            label={t("network.bonjour.label")}
          />
        </Row>
        {form.bonjour && form.mode !== "local" ? (
          <Row label={t("network.bonjour.hostName")} htmlFor="network-bonjour-host" hint={t("network.bonjour.hostNameHint", { host: data.bonjour_host_default })}>
            <div className="flex items-center gap-1.5">
              <TextInput
                id="network-bonjour-host"
                value={form.bonjourHost}
                placeholder={data.bonjour_host_default}
                disabled={data.fixed.bonjour}
                onChange={(e) => set({ bonjourHost: e.target.value })}
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
                className="w-48"
              />
              <span className="text-sm text-muted">.local</span>
            </div>
          </Row>
        ) : null}
        {form.bonjour && form.mode !== "local" ? (
            <div className="space-y-1.5">
              <h4 className="text-sm font-medium">{t("network.bonjour.announcedOn")}</h4>
              <p className="text-xs text-subtle">{t("network.bonjour.announcedOnHint")}</p>
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
          <h3 className="text-base font-semibold">{t("network.listening.title")}</h3>
          <p className="mt-0.5 text-sm text-muted">{t("network.listening.retries")}</p>
        </div>
        <ul className="space-y-1.5 text-sm">
          {APIS.filter(({ api }) => data[`${api}_on_web` as const]).map(({ api }) => (
            <li key={api} className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
              <CircleCheck className="size-3.5 shrink-0 text-success" />
              <span className="text-muted">{t("network.listening.onPleyPort", { name: listenerName(api) })}</span>
            </li>
          ))}
          {data.listeners.map((l) => (
            <li key={`${l.kind} ${l.address}`} className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
              {l.error ? <CircleAlert className="size-3.5 shrink-0 text-danger" /> : <CircleCheck className="size-3.5 shrink-0 text-success" />}
              <span className="font-mono text-xs">{l.address}</span>
              <span className="text-muted">{listenerName(l.kind)}</span>
              {l.error ? <span className="basis-full pl-5.5 text-xs text-danger">{t("network.listening.cantListen", { error: l.error })}</span> : null}
            </li>
          ))}
        </ul>
      </Card>

      {unusual && !fixedAll ? (
        <Note icon={<Info className="size-3.5" />}>{t("network.unusual", { addresses: [...data.web, ...data.jellyfin, ...data.plex].join(", ") })}</Note>
      ) : null}

      {/* Floats over the page only with something to save: at rest it would cover the rows on a phone. */}
      <div className={cn("flex justify-end", (edited || save.isPending) && "sticky bottom-4 z-10")}>
        <div className="flex max-w-full flex-wrap items-center justify-end gap-3 rounded-lg border border-border bg-surface-2/95 px-3 py-2 shadow-xl backdrop-blur">
          {edited && problem ? (
            <span className="text-xs text-danger">{problem}</span>
          ) : lost ? (
            <span className="text-xs text-danger">{t("network.wouldStop")}</span>
          ) : moved ? (
            <span className="text-xs text-muted">{t("network.pageMoves")}</span>
          ) : dirty ? (
            <span className="text-xs text-muted">{t("form.unsaved")}</span>
          ) : null}
          <Button variant="ghost" size="sm" disabled={!edited || save.isPending} onClick={() => setForm(formOf(data))}>
            {t("common:actions.reset")}
          </Button>
          <Button
            type="submit"
            variant="primary"
            size="sm"
            icon={<Save className="size-4" />}
            loading={save.isPending}
            disabled={!dirty || problem !== null}
          >
            {t("common:actions.save")}
          </Button>
        </div>
      </div>
    </form>
  );
}
