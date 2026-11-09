import { useState, type FormEvent, type ReactNode } from "react";
import { Navigate, useNavigate } from "react-router";
import { Trans, useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useNetwork, useRatingSystems, useSaveNetwork, useSaveSettings, useSettings, useTestService } from "@/api/hooks";
import type { ListenChoice, NetworkSettings, RatingSystems, Settings } from "@/api/types";
import { ErrorScreen, Splash, useAuth } from "@/auth";
import { CountrySelect } from "@/components/CountrySelect";
import { Button, Select, TextInput, Toggle } from "@/components/ui";
import { currentLanguage, deviceTags, type Language } from "@/i18n";
import { LanguageOptions } from "@/i18n/LanguageOptions";
import { regionOf, systemName } from "@/lib/access";
import { browserMetadataLanguage, metadataLanguageChoices, metadataLanguageName } from "@/lib/metadataLanguages";
import { useDocumentTitle } from "@/lib/hooks";
import { isLoopback, splitAddress } from "./settings/NetworkSettings";
import { Link, SecretInput } from "./settings/SettingsForm";
import { AuthLayout } from "./AuthLayout";

/**
 * The country and the language of titles and summaries to start from, from
 * the browser's languages (`tags`) where the server has neither yet. The
 * country is the one chosen, else the first of the browser's whose ratings
 * Pley knows ("en-NZ"), else the starting language's ("de-AT" is German,
 * so Germany). US English is the default nobody chose, so the browser's
 * language goes before it.
 */
export function startingChoices(
  settings: Settings,
  systems: RatingSystems | undefined,
  tags: readonly string[],
): { country: string; metadataLanguage: string } {
  const chosen = settings.metadata_language === "en-US" ? null : settings.metadata_language;
  const known = new Set(systems?.systems.map((s) => s.country));
  const browsers = tags.map((tag) => tag.split(/[-_]/)[1]?.toUpperCase()).find((c) => c !== undefined && known.has(c));
  const country = settings.country || browsers || regionOf(chosen ?? browserMetadataLanguage(tags, ""));
  return { country, metadataLanguage: chosen ?? browserMetadataLanguage(tags, country) };
}

/** Where Pley listens and announces now, to save again with one change. */
function choiceOf(network: NetworkSettings): ListenChoice {
  const { web, jellyfin, jellyfin_on_web, plex, plex_on_web, discovery, bonjour, bonjour_adapters, bonjour_host_name } = network;
  return { web, jellyfin, jellyfin_on_web, plex, plex_on_web, discovery, bonjour, bonjour_adapters, bonjour_host_name };
}

/**
 * The first run's second step (`/setup/server`), once the administrator's
 * account is made: what Pley will use for the server's name, Pley's
 * language, the language of titles and summaries, the country, a TMDB key,
 * and whether Apple's devices find it, each on a row of its own to confirm
 * or change. Everything starts filled in; each is in
 * Settings afterwards.
 */
export function SetupServerPage() {
  const user = useAuth();
  if (!user.is_admin) return <Navigate to="/" replace />;
  return <SetupServer />;
}

function SetupServer() {
  const { t } = useTranslation(["pages", "common"]);
  useDocumentTitle(t("setupServer.title"));
  const settings = useSettings();
  const network = useNetwork();
  const systems = useRatingSystems();

  if (settings.isPending || network.isPending || systems.isPending) return <Splash />;
  if (settings.isError) {
    return <ErrorScreen message={errorMessage(settings.error)} onRetry={() => void settings.refetch()} />;
  }
  // Without the network's settings or the countries, the rest still goes on.
  return <ServerForm settings={settings.data} network={network.data} systems={systems.data} />;
}

type RowKey = "name" | "language" | "metadata" | "country" | "tmdb" | "bonjour";

function ServerForm({
  settings,
  network,
  systems,
}: {
  settings: Settings;
  network: NetworkSettings | undefined;
  systems: RatingSystems | undefined;
}) {
  const { t } = useTranslation(["pages", "settings", "common"]);
  const navigate = useNavigate();
  const save = useSaveSettings();
  const saveNetwork = useSaveNetwork();
  const test = useTestService();
  const [name, setName] = useState(settings.server_name);
  const [language, setLanguage] = useState(settings.language || currentLanguage());
  const [start] = useState(() => startingChoices(settings, systems, deviceTags()));
  const [country, setCountry] = useState(start.country);
  const [metadataLanguage, setMetadataLanguage] = useState(start.metadataLanguage);
  const [tmdbKey, setTmdbKey] = useState(settings.tmdb_api_key);
  // Asked only where devices could reach Pley, and nothing has decided it
  // already; on unless Pley seems to be on Docker's own network.
  const askBonjour = network !== undefined && !network.fixed.bonjour && network.web.some((a) => !isLoopback(splitAddress(a).ip));
  const [bonjour, setBonjour] = useState(() => network !== undefined && (network.bonjour || !network.container_network));
  // Each row shows what Pley will use; Change opens its field in place.
  const [open, setOpen] = useState<ReadonlySet<RowKey>>(new Set());
  const openRow = (row: RowKey) => setOpen((rows) => new Set(rows).add(row));
  const [refused, setRefused] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const go = async (withoutKey: boolean) => {
    setError(null);
    const trimmed = name.trim();
    if (!trimmed) {
      openRow("name");
      return;
    }
    const key = tmdbKey.trim();
    const newKey = key !== "" && key !== settings.tmdb_api_key;
    setBusy(true);
    try {
      if (newKey && !withoutKey) {
        const result = await test.mutateAsync({ service: "tmdb", api_key: key }).catch(() => ({ ok: false }));
        if (!result.ok) {
          setRefused(true);
          openRow("tmdb");
          return;
        }
      }
      await save.mutateAsync({
        server_name: trimmed,
        language,
        metadata_language: metadataLanguage,
        country,
        tmdb_api_key: withoutKey ? settings.tmdb_api_key : key,
      });
      if (askBonjour && network && bonjour !== network.bonjour) await saveNetwork.mutateAsync({ ...choiceOf(network), bonjour });
      void navigate("/setup/start");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    void go(false);
  };

  const hasKey = tmdbKey.trim() !== "";
  return (
    <AuthLayout title={t("setupServer.title")} subtitle={t("setupServer.subtitle")}>
      <form onSubmit={onSubmit} className="space-y-5">
        <div className="divide-y divide-border border-y border-border">
          <Row
            id="setup-server-name"
            label={t("setupServer.name")}
            value={name}
            open={open.has("name")}
            onOpen={() => openRow("name")}
            hint={t("setupServer.nameHint")}
          >
            <TextInput id="setup-server-name" value={name} maxLength={64} required autoFocus onChange={(e) => setName(e.target.value)} />
          </Row>
          <Row
            id="setup-server-language"
            label={t("common:language.serverLabel")}
            value={t(`common:language.names.${language as Language}`)}
            open={open.has("language")}
            onOpen={() => openRow("language")}
            hint={t("setupServer.languageHint")}
          >
            <Select id="setup-server-language" value={language} autoFocus onChange={(e) => setLanguage(e.target.value)} className="w-full">
              <LanguageOptions />
            </Select>
          </Row>
          <Row
            id="setup-server-metadata"
            label={t("settings:server.metadata.language")}
            value={metadataLanguageName(metadataLanguage)}
            open={open.has("metadata")}
            onOpen={() => openRow("metadata")}
            hint={t("settings:server.metadata.languageHint")}
          >
            <Select id="setup-server-metadata" value={metadataLanguage} autoFocus onChange={(e) => setMetadataLanguage(e.target.value)} className="w-full">
              {metadataLanguageChoices(metadataLanguage).map((l) => (
                <option key={l.value} value={l.value}>
                  {l.label}
                </option>
              ))}
            </Select>
          </Row>
          <Row
            id="setup-server-country"
            label={t("settings:server.country")}
            value={systemName(country)}
            open={open.has("country")}
            onOpen={() => openRow("country")}
            hint={t("settings:server.countryHint")}
          >
            <CountrySelect id="setup-server-country" value={country} onChange={setCountry} className="w-full" />
          </Row>
          <Row
            id="setup-server-tmdb"
            label={t("setupServer.movieDetails")}
            value={hasKey ? t("setupServer.keyAdded") : t("setupServer.noKey")}
            note={
              hasKey ? null : (
                <>
                  {t("setupServer.keyNeeded")} <Link href="https://www.themoviedb.org/settings/api">{t("setupServer.getKey")}</Link>
                </>
              )
            }
            action={hasKey ? undefined : t("setupServer.addKey")}
            open={open.has("tmdb")}
            onOpen={() => openRow("tmdb")}
            error={refused ? t("setupServer.keyRefused") : null}
            hint={<Trans t={t} i18nKey="setupServer.tmdbHint" components={{ a: <Link href="https://www.themoviedb.org/settings/api" /> }} />}
          >
            <SecretInput
              id="setup-server-tmdb"
              what="key"
              value={tmdbKey}
              onChange={(v) => {
                setTmdbKey(v);
                setRefused(false);
              }}
            />
          </Row>
          {askBonjour && network ? (
            <Row
              id="setup-server-bonjour"
              label={t("setupServer.appleDevices")}
              value={bonjour ? t("setupServer.findIt") : t("setupServer.needAddress")}
              open={open.has("bonjour")}
              onOpen={() => openRow("bonjour")}
              hint={network.container_network ? t("settings:network.containerNetwork") : t("setupServer.bonjourHint")}
            >
              <div className="flex items-center justify-between gap-4">
                <span className="text-sm">{t("setupServer.bonjour")}</span>
                <Toggle checked={bonjour} onChange={setBonjour} label={t("setupServer.bonjour")} />
              </div>
            </Row>
          ) : null}
        </div>
        {error ? (
          <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
            {error}
          </p>
        ) : null}
        <div className="space-y-2">
          <Button type="submit" variant="primary" size="lg" className="w-full" loading={busy}>
            {t("common:actions.continue")}
          </Button>
          {refused ? (
            <Button type="button" variant="ghost" className="w-full" disabled={busy} onClick={() => void go(true)}>
              {t("setupServer.withoutKey")}
            </Button>
          ) : null}
        </div>
      </form>
    </AuthLayout>
  );
}

/**
 * One setting: what Pley will use (and a `note` for what isn't plain from
 * it), with Change (or `action`) beside it, or once opened its field,
 * labelled for `id`, with its hint.
 */
function Row({
  id,
  label,
  value,
  note,
  action,
  open,
  onOpen,
  hint,
  error,
  children,
}: {
  id: string;
  label: string;
  value: string;
  note?: ReactNode;
  action?: string;
  open: boolean;
  onOpen: () => void;
  hint: ReactNode;
  error?: string | null;
  children: ReactNode;
}) {
  const { t } = useTranslation("pages");
  if (open) {
    return (
      <div className="space-y-1.5 py-3">
        <label htmlFor={id} className="block text-xs font-medium text-muted">
          {label}
        </label>
        {children}
        {error ? <p className="text-xs text-danger">{error}</p> : <p className="text-xs text-subtle">{hint}</p>}
      </div>
    );
  }
  return (
    <div className="flex items-center gap-3 py-3">
      <div className="min-w-0 flex-1">
        <p className="text-xs font-medium text-muted">{label}</p>
        <p className="truncate text-sm text-fg">{value}</p>
        {note ? <p className="mt-1 text-xs text-subtle">{note}</p> : null}
      </div>
      <Button type="button" size="sm" variant="ghost" onClick={onOpen} aria-label={t("setupServer.changeWhat", { what: label })} className="shrink-0">
        {action ?? t("setupServer.change")}
      </Button>
    </div>
  );
}
