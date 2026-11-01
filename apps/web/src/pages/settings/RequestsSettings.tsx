import type { ReactNode } from "react";
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

const AVAILABILITY: { value: MinimumAvailability; label: string }[] = [
  { value: "announced", label: "Announced" },
  { value: "in_cinemas", label: "In cinemas" },
  { value: "released", label: "Released" },
];

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

function LimitInput({ id, value, onChange, unit }: { id: string; value: number; onChange: (v: number) => void; unit: string }) {
  return (
    <div className="flex items-center gap-2">
      <TextInput id={id} type="number" min={0} max={1000} value={value} onChange={(e) => onChange(limit(e.target.value))} className="w-28" />
      <span className="text-sm text-muted">{value === 0 ? "No limit" : unit}</span>
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
  const known = !!profiles?.some((p) => p.id === value);
  return (
    <Select id={id} aria-label={label} value={value} disabled={!profiles} onChange={(e) => onChange(Number(e.target.value))} className="w-full">
      <option value={0}>{none}</option>
      {value !== 0 && !known ? <option value={value}>{profiles ? `Profile ${value} (gone)` : `Profile ${value}`}</option> : null}
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
  const known = !!folders?.some((f) => f.path === value);
  return (
    <Select id={id} value={value} disabled={!folders} onChange={(e) => onChange(e.target.value)} className="w-full">
      <option value="">{none}</option>
      {value !== "" && !known ? <option value={value}>{folders ? `${value} (gone)` : value}</option> : null}
      {folders?.map((f) => (
        <option key={f.path} value={f.path}>
          {f.free_bytes != null ? `${f.path} (${formatBytes(f.free_bytes)} free)` : f.path}
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
  if (fresh && connect.isPending) status = <span className="text-subtle">Connecting to {name}…</span>;
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
  } else if (atSaved && savedOptions.isFetching) status = <span className="text-subtle">Asking {name}…</span>;
  else if (atSaved && savedOptions.isError) status = <Problem>{errorMessage(savedOptions.error)}</Problem>;
  else if (hasAddress && !needsKey) status = <span className="text-subtle">Connect to choose a quality profile and a folder.</span>;

  const profile = form[f.profile] ?? 0;
  const folder = form[f.folder] ?? "";

  return (
    <Group
      title={name}
      description={
        service === "radarr"
          ? "Movies approved are added to Radarr, which finds and downloads them."
          : "Shows approved are added to Sonarr with the seasons asked for, which it finds and downloads."
      }
    >
      <Row
        label="Address"
        htmlFor={`${service}-url`}
        hint={
          <>
            Where Pley reaches {name}, as {EXAMPLES[service]}. Empty: what's approved is added by hand.
          </>
        }
      >
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
        {savedUrl !== "" && !hasAddress && savedKey ? <Note>Saving without an address forgets the API key too.</Note> : null}
      </Row>
      <Row label="API key" htmlFor={`${service}-key`} hint={`In ${name}: Settings, General, Security.`}>
        <SecretInput
          id={`${service}-key`}
          what="key"
          value={typedKey ?? ""}
          onChange={setKey}
          placeholder={savedKey && !moved ? "Saved" : "Not set"}
          required={needsKey}
        />
        {needsKey ? <Note notice>Type {name}'s API key for this address: the saved one goes only to the address it was saved with.</Note> : null}
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
            Connect
          </Button>
          <div role="status" className="min-w-0 pt-1 text-xs">
            {status}
          </div>
        </div>
      </Row>
      {hasAddress ? (
        <>
          <Row label="Quality profile" htmlFor={`${service}-profile`}>
            <ProfileSelect
              id={`${service}-profile`}
              value={profile}
              onChange={(v) => set(f.profile, v)}
              profiles={options?.quality_profiles}
              none="None chosen"
            />
            {profile === 0 ? <Note notice>Approving fails until one is chosen.</Note> : null}
          </Row>
          <Row label="Root folder" htmlFor={`${service}-folder`}>
            <FolderSelect
              id={`${service}-folder`}
              value={folder}
              onChange={(v) => set(f.folder, v)}
              folders={options?.root_folders}
              none="None chosen"
            />
            {folder === "" ? <Note notice>Approving fails until one is chosen.</Note> : null}
          </Row>
          {service === "radarr" ? (
            <Row label="Minimum availability" htmlFor="radarr-availability" hint="When Radarr may take a release of a movie asked for.">
              <Select
                id="radarr-availability"
                value={form.radarr_minimum_availability ?? "released"}
                onChange={(e) => set("radarr_minimum_availability", e.target.value as MinimumAvailability)}
                className="w-full"
              >
                {AVAILABILITY.map((a) => (
                  <option key={a.value} value={a.value}>
                    {a.label}
                  </option>
                ))}
              </Select>
            </Row>
          ) : null}
          {service === "sonarr" && (options?.language_profiles.length || form.sonarr_language_profile) ? (
            <Row label="Language profile" htmlFor="sonarr-language" hint="Sonarr 3's. Sonarr 4 has none.">
              <ProfileSelect
                id="sonarr-language"
                value={form.sonarr_language_profile ?? 0}
                onChange={(v) => set("sonarr_language_profile", v)}
                profiles={options?.language_profiles}
                none="Sonarr's first"
              />
            </Row>
          ) : null}
          <Row label="Anime quality profile" htmlFor={`${service}-anime-profile`} hint="For titles TMDB counts as anime.">
            <ProfileSelect
              id={`${service}-anime-profile`}
              value={form[f.animeProfile] ?? 0}
              onChange={(v) => set(f.animeProfile, v)}
              profiles={options?.quality_profiles}
              none="The quality profile above"
            />
          </Row>
          <Row label="Anime root folder" htmlFor={`${service}-anime-folder`}>
            <FolderSelect
              id={`${service}-anime-folder`}
              value={form[f.animeFolder] ?? ""}
              onChange={(v) => set(f.animeFolder, v)}
              folders={options?.root_folders}
              none="The root folder above"
            />
          </Row>
          {service === "sonarr" ? (
            <Row label="Season folders" hint="A folder for each season of a show.">
              <Toggle checked={form.sonarr_season_folders ?? true} onChange={(v) => set("sonarr_season_folders", v)} label="Season folders" />
            </Row>
          ) : null}
          <Row
            label="Search when added"
            hint={service === "radarr" ? "Look for the movie as soon as it's approved." : "Look for the seasons asked for as soon as they're approved."}
          >
            <Toggle checked={form[f.search] ?? true} onChange={(v) => set(f.search, v)} label={`${name}: search when added`} />
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
  useDocumentTitle("Requests");
  const draft = useSettingsForm(KEYS);
  const qc = useQueryClient();
  const onSaved = () => {
    // Discover and Requests in the menu follow `requests`; what Radarr and Sonarr offer, their addresses.
    void qc.invalidateQueries({ queryKey: requestKeys.all });
    void qc.invalidateQueries({ queryKey: ["settings", "arr"] });
  };

  return (
    <SettingsForm draft={draft} title="Requests" onSaved={onSaved}>
      {(form) => {
        if (form.requests === undefined) {
          return <EmptyState icon={<Inbox />} title="This server doesn't take requests" description="Update Pley to let people ask for movies and shows." />;
        }
        const hasTmdb = form.tmdb_api_key.trim() !== "";
        return (
          <>
            <Group title="Requests" description="People ask for movies and shows the libraries don't have, found on TMDB. You approve or decline each.">
              <Row label="Take requests" hint="Discover and Requests show in the menu of every account that may ask.">
                <Toggle checked={form.requests} disabled={!hasTmdb && !form.requests} onChange={(v) => draft.set("requests", v)} label="Take requests" />
                {!hasTmdb ? (
                  <Note notice={form.requests}>
                    Needs a TMDB API key: add one in{" "}
                    <RouterLink to="/settings/server#tmdb-key" className="text-accent hover:underline">
                      Server
                    </RouterLink>
                    .
                  </Note>
                ) : null}
              </Row>
              <Row label="Movies per 7 days" htmlFor="request-limit-movies" hint="How many each account may ask for in any 7 days. Administrators have no limit.">
                <LimitInput id="request-limit-movies" value={form.request_limit_movies ?? 0} onChange={(v) => draft.set("request_limit_movies", v)} unit="movies" />
              </Row>
              <Row label="Seasons per 7 days" htmlFor="request-limit-seasons" hint="A show counts its seasons.">
                <LimitInput id="request-limit-seasons" value={form.request_limit_seasons ?? 0} onChange={(v) => draft.set("request_limit_seasons", v)} unit="seasons" />
              </Row>
              <p className="text-xs text-subtle">
                Who may ask, and whose requests are approved as they're made, is up to each account in{" "}
                <RouterLink to="/settings/users" className="text-accent hover:underline">
                  Users
                </RouterLink>
                . Administrators' are always approved.
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
