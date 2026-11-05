import type { ReactNode } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Link as RouterLink } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { CircleAlert, CircleCheck, Inbox, Plug } from "lucide-react";
import { errorMessage } from "@/api/client";
import { requestKeys, useArrOptions, useConnectArr, type ArrOptions, type ArrService } from "@/api/requests";
import type { MinimumAvailability, Settings } from "@/api/types";
import { Button, EmptyState, Select, TextInput, Toggle } from "@/components/ui";
import { formatBytes } from "@/lib/format";
import { useDocumentTitle } from "@/lib/hooks";
import { Group, Note, Row, SecretInput, SettingsForm, useSettingsForm } from "./SettingsForm";

/** The settings this page edits. */
const KEYS = [
  "requests",
  "request_limit_movies",
  "request_limit_seasons",
  "radarr_url",
  "radarr_api_key",
  "radarr_quality_profile",
  "radarr_root_folder",
  "radarr_anime_quality_profile",
  "radarr_anime_root_folder",
  "radarr_minimum_availability",
  "radarr_search",
  "sonarr_url",
  "sonarr_api_key",
  "sonarr_quality_profile",
  "sonarr_root_folder",
  "sonarr_language_profile",
  "sonarr_anime_quality_profile",
  "sonarr_anime_root_folder",
  "sonarr_season_folders",
  "sonarr_search",
] as const satisfies readonly (keyof Settings)[];

/** Each service's settings. */
const FIELDS = {
  radarr: {
    url: "radarr_url",
    key: "radarr_api_key",
    keySet: "radarr_api_key_set",
    profile: "radarr_quality_profile",
    folder: "radarr_root_folder",
    animeProfile: "radarr_anime_quality_profile",
    animeFolder: "radarr_anime_root_folder",
    search: "radarr_search",
  },
  sonarr: {
    url: "sonarr_url",
    key: "sonarr_api_key",
    keySet: "sonarr_api_key_set",
    profile: "sonarr_quality_profile",
    folder: "sonarr_root_folder",
    animeProfile: "sonarr_anime_quality_profile",
    animeFolder: "sonarr_anime_root_folder",
    search: "sonarr_search",
  },
} as const satisfies Record<ArrService, Record<string, keyof Settings>>;

const NAMES: Record<ArrService, string> = { radarr: "Radarr", sonarr: "Sonarr" };
const EXAMPLES: Record<ArrService, string> = { radarr: "http://localhost:7878", sonarr: "http://localhost:8989" };

const AVAILABILITY: MinimumAvailability[] = ["announced", "in_cinemas", "released"];

/** An address as the server keeps it: trimmed, as URLs are written ("HTTP://Host/" is "http://host"), less a trailing `/`. */
function address(url: string | undefined): string {
  const value = (url ?? "").trim();
  if (!value) return "";
  let written = value;
  try {
    written = new URL(value).href;
  } catch {
    // Not an address: the server says so on saving.
  }
  return written.replace(/\/+$/, "");
}

/** 0–1000; 0 is no limit. */
function limit(raw: string): number {
  return Math.min(1000, Math.max(0, Math.floor(Number(raw) || 0)));
}

type Draft = ReturnType<typeof useSettingsForm>;

function LimitInput({ id, value, onChange, unit }: { id: string; value: number; onChange: (v: number) => void; unit: "movieUnit" | "seasonUnit" }) {
  const { t } = useTranslation("settings");
  return (
    <div className="flex items-center gap-2">
      <TextInput id={id} type="number" min={0} max={1000} value={value} onChange={(e) => onChange(limit(e.target.value))} className="w-28" />
      <span className="text-sm text-muted">{value === 0 ? t("requests.noLimit") : t(`requests.${unit}`, { count: value })}</span>
    </div>
  );
}

/** Radarr's or Sonarr's quality profiles; one saved that it no longer has stays listed, so it shows. */
function ProfileSelect({
  id,
  value,
  onChange,
  profiles,
  none,
  label,
}: {
  id: string;
  value: number;
  onChange: (v: number) => void;
  profiles: ArrOptions["quality_profiles"] | undefined;
  /** What 0 stands for. */
  none: string;
  label?: string;
}) {
  const { t } = useTranslation("settings");
  const known = !!profiles?.some((p) => p.id === value);
  return (
    <Select id={id} aria-label={label} value={value} disabled={!profiles} onChange={(e) => onChange(Number(e.target.value))} className="w-full">
      <option value={0}>{none}</option>
      {value !== 0 && !known ? (
        <option value={value}>{t(profiles ? "requests.arr.profileGone" : "requests.arr.profile", { id: value })}</option>
      ) : null}
      {profiles?.map((p) => (
        <option key={p.id} value={p.id}>
          {p.name}
        </option>
      ))}
    </Select>
  );
}

/** Root folders with the space left in each, as with the profiles. */
function FolderSelect({
  id,
  value,
  onChange,
  folders,
  none,
}: {
  id: string;
  value: string;
  onChange: (v: string) => void;
  folders: ArrOptions["root_folders"] | undefined;
  none: string;
}) {
  const { t } = useTranslation("settings");
  const known = !!folders?.some((f) => f.path === value);
  return (
    <Select id={id} value={value} disabled={!folders} onChange={(e) => onChange(e.target.value)} className="w-full">
      <option value="">{none}</option>
      {value !== "" && !known ? <option value={value}>{folders ? t("requests.arr.folderGone", { path: value }) : value}</option> : null}
      {folders?.map((f) => (
        <option key={f.path} value={f.path}>
          {f.free_bytes != null ? t("requests.arr.folderFree", { path: f.path, size: formatBytes(f.free_bytes) }) : f.path}
        </option>
      ))}
    </Select>
  );
}

/**
 * Radarr or Sonarr: its address and key, Connect (with what the form holds,
 * not saved yet), and what requests are sent with, from what it offers.
 */
function ArrGroup({ service, form, draft }: { service: ArrService; form: Settings; draft: Draft }) {
  const { t } = useTranslation("settings");
  const f = FIELDS[service];
  const name = NAMES[service];
  const saved = draft.settings.data;
  const savedUrl = address(saved?.[f.url]);
  const savedKey = !!saved?.[f.keySet];
  const url = form[f.url] ?? "";
  const typedKey = form[f.key];
  const moved = address(url) !== savedUrl;
  const atSaved = !moved && !typedKey;
  const hasAddress = address(url) !== "";
  // The saved key goes only to the address it was saved with: another one
  // needs its key typed (the server won't save it without), and no address
  // forgets the key with it.
  const needsKey = moved && hasAddress && savedKey && !typedKey;
  const savedOptions = useArrOptions(service, savedUrl !== "" && savedKey);
  const connect = useConnectArr();
  // An answer about what the form held when Connect was pressed, while it still holds that.
  const fresh = connect.variables?.url === address(url) && connect.variables.api_key === typedKey;
  const options: ArrOptions | undefined = fresh && connect.data ? connect.data : atSaved ? savedOptions.data : undefined;
  const { set } = draft;
  // Only a key typed is sent: left out, the saved one stays.
  const setKey = (value: string) => set(f.key, value === "" ? undefined : value);

  let status: ReactNode = null;
  if (fresh && connect.isPending) status = <span className="text-subtle">{t("requests.arr.connecting", { name })}</span>;
  else if (fresh && connect.isError) status = <Problem>{errorMessage(connect.error)}</Problem>;
  else if (options) {
    status = (
      <span className="flex items-start gap-1.5 text-success">
        <CircleCheck className="mt-px size-3.5 shrink-0" />
        <span>
          {name} {options.version}
        </span>
      </span>
    );
  } else if (atSaved && savedOptions.isFetching) status = <span className="text-subtle">{t("requests.arr.asking", { name })}</span>;
  else if (atSaved && savedOptions.isError) status = <Problem>{errorMessage(savedOptions.error)}</Problem>;
  else if (hasAddress && !needsKey) status = <span className="text-subtle">{t("requests.arr.connectToChoose")}</span>;

  const profile = form[f.profile] ?? 0;
  const folder = form[f.folder] ?? "";

  return (
    <Group
      title={name}
      description={t(`requests.arr.${service}`)}
    >
      <Row label={t("requests.arr.address")} htmlFor={`${service}-url`} hint={t("requests.arr.addressHint", { name, example: EXAMPLES[service] })}>
        <TextInput
          id={`${service}-url`}
          type="url"
          inputMode="url"
          autoComplete="off"
          spellCheck={false}
          placeholder={EXAMPLES[service]}
          value={url}
          onChange={(e) => set(f.url, e.target.value)}
        />
        {savedUrl !== "" && !hasAddress && savedKey ? <Note>{t("requests.arr.forgetsKey")}</Note> : null}
      </Row>
      <Row label={t("requests.arr.apiKey")} htmlFor={`${service}-key`} hint={t("requests.arr.apiKeyHint", { name })}>
        <SecretInput
          id={`${service}-key`}
          what="key"
          value={typedKey ?? ""}
          onChange={setKey}
          placeholder={savedKey && !moved ? t("requests.arr.keySaved") : t("form.notSet")}
          required={needsKey}
        />
        {needsKey ? <Note notice>{t("requests.arr.typeKey", { name })}</Note> : null}
        <div className="mt-2 flex min-w-0 items-start gap-3">
          <Button
            size="sm"
            variant="outline"
            icon={<Plug className="size-3.5" />}
            loading={fresh && connect.isPending}
            disabled={!hasAddress || needsKey}
            className="shrink-0"
            onClick={() => connect.mutate({ service, url: address(url), ...(typedKey ? { api_key: typedKey } : {}) })}
          >
            {t("requests.arr.connect")}
          </Button>
          <div role="status" className="min-w-0 pt-1 text-xs">
            {status}
          </div>
        </div>
      </Row>
      {hasAddress ? (
        <>
          <Row label={t("requests.arr.qualityProfile")} htmlFor={`${service}-profile`}>
            <ProfileSelect
              id={`${service}-profile`}
              value={profile}
              onChange={(v) => set(f.profile, v)}
              profiles={options?.quality_profiles}
              none={t("requests.arr.noneChosen")}
            />
            {profile === 0 ? <Note notice>{t("requests.arr.approvingFails")}</Note> : null}
          </Row>
          <Row label={t("requests.arr.rootFolder")} htmlFor={`${service}-folder`}>
            <FolderSelect
              id={`${service}-folder`}
              value={folder}
              onChange={(v) => set(f.folder, v)}
              folders={options?.root_folders}
              none={t("requests.arr.noneChosen")}
            />
            {folder === "" ? <Note notice>{t("requests.arr.approvingFails")}</Note> : null}
          </Row>
          {service === "radarr" ? (
            <Row label={t("requests.arr.minimumAvailability")} htmlFor="radarr-availability" hint={t("requests.arr.minimumAvailabilityHint")}>
              <Select
                id="radarr-availability"
                value={form.radarr_minimum_availability ?? "released"}
                onChange={(e) => set("radarr_minimum_availability", e.target.value as MinimumAvailability)}
                className="w-full"
              >
                {AVAILABILITY.map((a) => (
                  <option key={a} value={a}>
                    {t(`requests.arr.availability.${a}`)}
                  </option>
                ))}
              </Select>
            </Row>
          ) : null}
          {service === "sonarr" && (options?.language_profiles.length || form.sonarr_language_profile) ? (
            <Row label={t("requests.arr.languageProfile")} htmlFor="sonarr-language" hint={t("requests.arr.languageProfileHint")}>
              <ProfileSelect
                id="sonarr-language"
                value={form.sonarr_language_profile ?? 0}
                onChange={(v) => set("sonarr_language_profile", v)}
                profiles={options?.language_profiles}
                none={t("requests.arr.sonarrFirst")}
              />
            </Row>
          ) : null}
          <Row label={t("requests.arr.animeProfile")} htmlFor={`${service}-anime-profile`} hint={t("requests.arr.animeProfileHint")}>
            <ProfileSelect
              id={`${service}-anime-profile`}
              value={form[f.animeProfile] ?? 0}
              onChange={(v) => set(f.animeProfile, v)}
              profiles={options?.quality_profiles}
              none={t("requests.arr.profileAbove")}
            />
          </Row>
          <Row label={t("requests.arr.animeFolder")} htmlFor={`${service}-anime-folder`}>
            <FolderSelect
              id={`${service}-anime-folder`}
              value={form[f.animeFolder] ?? ""}
              onChange={(v) => set(f.animeFolder, v)}
              folders={options?.root_folders}
              none={t("requests.arr.folderAbove")}
            />
          </Row>
          {service === "sonarr" ? (
            <Row label={t("requests.arr.seasonFolders")} hint={t("requests.arr.seasonFoldersHint")}>
              <Toggle checked={form.sonarr_season_folders ?? true} onChange={(v) => set("sonarr_season_folders", v)} label={t("requests.arr.seasonFolders")} />
            </Row>
          ) : null}
          <Row label={t("requests.arr.search")} hint={service === "radarr" ? t("requests.arr.searchMovie") : t("requests.arr.searchSeasons")}>
            <Toggle checked={form[f.search] ?? true} onChange={(v) => set(f.search, v)} label={t("requests.arr.searchToggle", { name })} />
          </Row>
        </>
      ) : null}
    </Group>
  );
}

function Problem({ children }: { children: ReactNode }) {
  return (
    <span className="flex items-start gap-1.5 text-danger">
      <CircleAlert className="mt-px size-3.5 shrink-0" />
      <span>{children}</span>
    </span>
  );
}

/** Settings → Requests: taking them, how many each account may make, and Radarr and Sonarr. */
export function RequestsSettings() {
  const { t } = useTranslation("settings");
  useDocumentTitle(t("requests.title"));
  const draft = useSettingsForm(KEYS);
  const qc = useQueryClient();
  const onSaved = () => {
    // Discover and Requests in the menu follow `requests`; what Radarr and Sonarr offer, their addresses.
    void qc.invalidateQueries({ queryKey: requestKeys.all });
    void qc.invalidateQueries({ queryKey: ["settings", "arr"] });
  };

  return (
    <SettingsForm draft={draft} title={t("requests.title")} onSaved={onSaved}>
      {(form) => {
        if (form.requests === undefined) {
          return <EmptyState icon={<Inbox />} title={t("requests.unsupported")} description={t("requests.unsupportedHint")} />;
        }
        const hasTmdb = form.tmdb_api_key.trim() !== "";
        return (
          <>
            <Group title={t("requests.title")} description={t("requests.description")}>
              <Row label={t("requests.take")} hint={t("requests.takeHint")}>
                <Toggle checked={form.requests} disabled={!hasTmdb && !form.requests} onChange={(v) => draft.set("requests", v)} label={t("requests.take")} />
                {!hasTmdb ? (
                  <Note notice={form.requests}>
                    <Trans
                      t={t}
                      i18nKey="requests.needsTmdb"
                      components={{ a: <RouterLink to="/settings/server#tmdb-key" className="text-accent hover:underline" /> }}
                    />
                  </Note>
                ) : null}
              </Row>
              <Row label={t("requests.movies")} htmlFor="request-limit-movies" hint={t("requests.moviesHint")}>
                <LimitInput
                  id="request-limit-movies"
                  value={form.request_limit_movies ?? 0}
                  onChange={(v) => draft.set("request_limit_movies", v)}
                  unit="movieUnit"
                />
              </Row>
              <Row label={t("requests.seasons")} htmlFor="request-limit-seasons" hint={t("requests.seasonsHint")}>
                <LimitInput
                  id="request-limit-seasons"
                  value={form.request_limit_seasons ?? 0}
                  onChange={(v) => draft.set("request_limit_seasons", v)}
                  unit="seasonUnit"
                />
              </Row>
              <p className="text-xs text-subtle">
                <Trans t={t} i18nKey="requests.whoMayAsk" components={{ a: <RouterLink to="/settings/users" className="text-accent hover:underline" /> }} />
              </p>
            </Group>
            <ArrGroup service="radarr" form={form} draft={draft} />
            <ArrGroup service="sonarr" form={form} draft={draft} />
          </>
        );
      }}
    </SettingsForm>
  );
}
