import { Link as RouterLink } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { appKeys } from "@/api/apps";
import { keys, useSettings, useSystemInfo, useUsers, useWatchingHours } from "@/api/hooks";
import type { LibraryWork, Settings, TvMetadata } from "@/api/types";
import { useAuth } from "@/auth";
import { Select, TextInput, Toggle } from "@/components/ui";
import { LanguageOptions } from "@/i18n/LanguageOptions";
import { Trans, useTranslation } from "react-i18next";
import { formatTimestamp } from "@/lib/format";
import { useDocumentTitle } from "@/lib/hooks";
import { Group, Link, Note, Row, SecretInput, SettingsForm, TestButton, useSettingsForm } from "./SettingsForm";
import { knownTranscoder } from "./transcodeChoices";
import { hourOfDay, WatchingHoursChart } from "./WatchingHoursChart";

/** The settings this page edits (the transcoder's are on their own page). */
const KEYS = [
  "server_name",
  "language",
  "tmdb_api_key",
  "metadata_language",
  "tv_metadata",
  "tvdb_api_key",
  "tvdb_pin",
  "music_metadata",
  "scan_interval_minutes",
  "watch_libraries",
  "detect_markers",
  "seek_previews",
  "previews_in_background",
  "library_work",
  "library_work_from",
  "library_work_to",
  "library_work_max_cpu",
  "third_party_apps",
  "require_two_step",
  "opensubtitles_api_key",
  "opensubtitles_username",
  "opensubtitles_password",
] as const satisfies readonly (keyof Settings)[];

const LANGUAGES = [
  "en-US",
  "en-GB",
  "fr-FR",
  "de-DE",
  "es-ES",
  "es-MX",
  "it-IT",
  "pt-BR",
  "pt-PT",
  "nl-NL",
  "sv-SE",
  "da-DK",
  "fi-FI",
  "nb-NO",
  "pl-PL",
  "cs-CZ",
  "ru-RU",
  "uk-UA",
  "tr-TR",
  "ar-SA",
  "he-IL",
  "hi-IN",
  "ja-JP",
  "ko-KR",
  "zh-CN",
  "zh-TW",
];

const TV_SOURCES: Record<Exclude<TvMetadata, "auto">, string> = { tmdb: "TMDB", tvdb: "TheTVDB", tvmaze: "TVmaze" };

/** The TV source "Automatic" stands for with these keys (the server decides the same way). */
function automaticTvSource(form: Settings): Exclude<TvMetadata, "auto"> {
  if (form.tmdb_api_key.trim()) return "tmdb";
  if (form.tvdb_api_key.trim()) return "tvdb";
  return "tvmaze";
}

export function ServerSettings() {
  const { t } = useTranslation(["settings", "common"]);
  useDocumentTitle(t("server.documentTitle"));
  const draft = useSettingsForm(KEYS);
  const system = useSystemInfo();
  const qc = useQueryClient();
  const { set, setForm } = draft;
  // A source can't stay chosen once its key is gone.
  const setKey = (key: "tmdb_api_key" | "tvdb_api_key", value: string) =>
    setForm((f) => {
      if (!f) return f;
      const source: TvMetadata = key === "tmdb_api_key" ? "tmdb" : "tvdb";
      const orphaned = !value.trim() && f.tv_metadata === source;
      return { ...f, [key]: value, ...(orphaned ? { tv_metadata: "auto" as const } : {}) };
    });
  const onSaved = (changes: Partial<Settings>) => {
    // Turning apps by other publishers off signed them out.
    if (changes.third_party_apps === false) {
      void qc.invalidateQueries({ queryKey: appKeys.mine });
      void qc.invalidateQueries({ queryKey: ["users"] });
    }
    // The pages before signing in, and accounts without their own, show the server's language.
    if (changes.language !== undefined) void qc.invalidateQueries({ queryKey: keys.server });
    // Requiring two-step sign-in signed out everyone without it.
    if (changes.require_two_step !== undefined) {
      void qc.invalidateQueries({ queryKey: keys.server });
      void qc.invalidateQueries({ queryKey: ["users"] });
    }
    // The hours library work may begin in follow its settings.
    if (Object.keys(changes).some((k) => k.startsWith("library_work"))) {
      void qc.invalidateQueries({ queryKey: ["library-work"] });
    }
  };

  return (
    <SettingsForm draft={draft} title={t("server.title")} onSaved={onSaved}>
      {(form) => {
        const hasTmdb = form.tmdb_api_key.trim() !== "";
        const hasTvdb = form.tvdb_api_key.trim() !== "";
        const noChromaprint = knownTranscoder(system.data)?.chromaprint === false;
        return (
          <>
            <Group title={t("server.general")}>
              <Row label={t("server.serverName")} hint={t("server.serverNameHint")} htmlFor="server-name">
                <TextInput id="server-name" value={form.server_name} onChange={(e) => set("server_name", e.target.value)} />
              </Row>
              <Row label={t("common:language.serverLabel")} hint={t("common:language.serverHint")} htmlFor="server-language">
                {/* English until an administrator picks another (an empty setting). */}
                <Select id="server-language" value={form.language || "en"} onChange={(e) => set("language", e.target.value)}>
                  <LanguageOptions />
                </Select>
              </Row>
            </Group>

            <Group title={t("server.metadata.title")} description={t("server.metadata.description")}>
              <Row
                label={t("server.metadata.tmdbKey")}
                htmlFor="tmdb-key"
                hint={<Trans t={t} i18nKey="server.metadata.tmdbKeyHint" components={{ a: <Link href="https://www.themoviedb.org/settings/api" /> }} />}
              >
                <SecretInput id="tmdb-key" what="key" value={form.tmdb_api_key} onChange={(v) => setKey("tmdb_api_key", v)} />
                <TestButton key={form.tmdb_api_key} label={t("server.metadata.testTmdb")} test={{ service: "tmdb", api_key: form.tmdb_api_key }} />
              </Row>
              <Row
                label={t("server.metadata.tvShows")}
                htmlFor="tv-source"
                hint={<Trans t={t} i18nKey="server.metadata.tvShowsHint" components={{ a: <Link href="https://www.tvmaze.com" /> }} />}
              >
                <Select id="tv-source" value={form.tv_metadata} onChange={(e) => set("tv_metadata", e.target.value as TvMetadata)} className="w-full">
                  <option value="auto">{t("server.metadata.automatic", { source: TV_SOURCES[automaticTvSource(form)] })}</option>
                  <option value="tmdb" disabled={!hasTmdb}>
                    {hasTmdb ? TV_SOURCES.tmdb : t("server.metadata.needsKey", { source: TV_SOURCES.tmdb })}
                  </option>
                  <option value="tvdb" disabled={!hasTvdb}>
                    {hasTvdb ? TV_SOURCES.tvdb : t("server.metadata.needsKey", { source: TV_SOURCES.tvdb })}
                  </option>
                  <option value="tvmaze">{t("server.metadata.tvmazeNoKey")}</option>
                </Select>
                <TestButton label={t("server.metadata.testTvmaze")} test={{ service: "tvmaze" }} />
              </Row>
              <Row
                label={t("server.metadata.tvdbKey")}
                htmlFor="tvdb-key"
                hint={<Trans t={t} i18nKey="server.metadata.tvdbKeyHint" components={{ a: <Link href="https://thetvdb.com/api-information" /> }} />}
              >
                <SecretInput id="tvdb-key" what="key" value={form.tvdb_api_key} onChange={(v) => setKey("tvdb_api_key", v)} />
              </Row>
              <Row label={t("server.metadata.tvdbPin")} htmlFor="tvdb-pin" hint={t("server.metadata.tvdbPinHint")}>
                <SecretInput id="tvdb-pin" what="pin" value={form.tvdb_pin} onChange={(v) => set("tvdb_pin", v)} />
                <TestButton
                  key={`${form.tvdb_api_key}
${form.tvdb_pin}`}
                  label={t("server.metadata.testTvdb")}
                  test={{ service: "tvdb", api_key: form.tvdb_api_key, pin: form.tvdb_pin }}
                />
              </Row>
              <Row
                label={t("server.metadata.music")}
                hint={<Trans t={t} i18nKey="server.metadata.musicHint" components={{ a: <Link href="https://musicbrainz.org" /> }} />}
              >
                <Toggle checked={form.music_metadata} onChange={(v) => set("music_metadata", v)} label={t("server.metadata.musicToggle")} />
                <TestButton label={t("server.metadata.testMusicBrainz")} test={{ service: "musicbrainz" }} />
              </Row>
              <Row label={t("server.metadata.language")} htmlFor="meta-lang" hint={t("server.metadata.languageHint")}>
                <Select id="meta-lang" value={form.metadata_language} onChange={(e) => set("metadata_language", e.target.value)} className="w-full">
                  {(LANGUAGES.includes(form.metadata_language) ? LANGUAGES : [form.metadata_language, ...LANGUAGES]).map((l) => (
                    <option key={l} value={l}>
                      {l}
                    </option>
                  ))}
                </Select>
              </Row>
              <p className="text-xs text-subtle">
                <Trans
                  t={t}
                  i18nKey="server.metadata.attribution"
                  components={{ tvdb: <Link href="https://thetvdb.com" />, license: <Link href="https://creativecommons.org/licenses/by-sa/4.0/" /> }}
                />
              </p>
            </Group>

            <Group title={t("server.subtitles.title")} description={t("server.subtitles.description")}>
              <Row
                label={t("server.subtitles.key")}
                htmlFor="opensubtitles-key"
                hint={<Trans t={t} i18nKey="server.subtitles.keyHint" components={{ a: <Link href="https://www.opensubtitles.com/consumers" /> }} />}
              >
                <SecretInput
                  id="opensubtitles-key"
                  what="key"
                  value={form.opensubtitles_api_key ?? ""}
                  onChange={(v) => set("opensubtitles_api_key", v)}
                />
              </Row>
              <Row label={t("server.subtitles.account")} htmlFor="opensubtitles-username" hint={t("server.subtitles.accountHint")}>
                <div className="grid gap-2 sm:grid-cols-2">
                  <TextInput
                    id="opensubtitles-username"
                    aria-label={t("server.subtitles.username")}
                    autoComplete="off"
                    spellCheck={false}
                    placeholder={t("server.subtitles.usernamePlaceholder")}
                    value={form.opensubtitles_username ?? ""}
                    onChange={(e) => {
                      const username = e.target.value;
                      // Another account's password isn't this one's (the server forgets it too).
                      setForm((f) =>
                        f ? { ...f, opensubtitles_username: username, opensubtitles_password: undefined, opensubtitles_password_set: false } : f,
                      );
                    }}
                  />
                  <TextInput
                    id="opensubtitles-password"
                    aria-label={t("server.subtitles.password")}
                    type="password"
                    autoComplete="new-password"
                    placeholder={form.opensubtitles_password_set ? t("server.subtitles.passwordSaved") : t("server.subtitles.passwordPlaceholder")}
                    disabled={!form.opensubtitles_username?.trim()}
                    value={form.opensubtitles_password ?? ""}
                    // Left empty, the saved password stays.
                    onChange={(e) => set("opensubtitles_password", e.target.value || undefined)}
                  />
                </div>
                <TestButton
                  key={`${form.opensubtitles_api_key ?? ""}
${form.opensubtitles_username ?? ""}
${form.opensubtitles_password ?? ""}`}
                  label={t("server.subtitles.test")}
                  test={{
                    service: "opensubtitles",
                    api_key: form.opensubtitles_api_key ?? "",
                    username: form.opensubtitles_username ?? "",
                    ...(form.opensubtitles_password ? { password: form.opensubtitles_password } : {}),
                  }}
                />
              </Row>
            </Group>

            <Group title={t("server.scanning.title")}>
              <Row label={t("server.scanning.watch")} hint={t("server.scanning.watchHint")}>
                <Toggle checked={form.watch_libraries} onChange={(v) => set("watch_libraries", v)} label={t("server.scanning.watchToggle")} />
              </Row>
              <Row label={t("server.scanning.markers")} hint={t("server.scanning.markersHint")}>
                <Toggle
                  checked={form.detect_markers}
                  disabled={noChromaprint}
                  onChange={(v) => set("detect_markers", v)}
                  label={t("server.scanning.markers")}
                />
                {noChromaprint ? <Note>{t("server.scanning.noChromaprint")}</Note> : null}
              </Row>
              <Row label={t("server.scanning.seekPreviews")} hint={t("server.scanning.seekPreviewsHint")}>
                <Toggle checked={form.seek_previews} onChange={(v) => set("seek_previews", v)} label={t("server.scanning.seekPreviews")} />
              </Row>
              <Row label={t("server.scanning.wholeLibraries")} hint={t("server.scanning.wholeLibrariesHint")}>
                <Toggle
                  checked={form.previews_in_background}
                  disabled={!form.seek_previews}
                  onChange={(v) => set("previews_in_background", v)}
                  label={t("server.scanning.wholeLibraries")}
                />
                {form.seek_previews ? null : <Note>{t("server.scanning.needsSeekPreviews")}</Note>}
              </Row>
              <LibraryWorkRows form={form} set={set} />
              <Row label={t("server.scanning.interval")} htmlFor="scan-interval" hint={t("server.scanning.intervalHint")}>
                <div className="flex items-center gap-2">
                  <TextInput
                    id="scan-interval"
                    type="number"
                    min={0}
                    value={form.scan_interval_minutes}
                    onChange={(e) => set("scan_interval_minutes", Math.max(0, Number(e.target.value) || 0))}
                    className="w-28"
                  />
                  <span className="text-sm text-muted">{t("server.scanning.minutes")}</span>
                </div>
              </Row>
            </Group>

            <Group title={t("server.signIn.title")}>
              <RequireTwoStepRow
                checked={form.require_two_step ?? false}
                onChange={(v) => set("require_two_step", v)}
                known={form.require_two_step !== undefined}
              />
            </Group>

            <Group title={t("server.apps.title")}>
              <Row label={t("server.apps.thirdParty")} hint={t("server.apps.thirdPartyHint")}>
                {/* Off until a server that knows the setting sends it. */}
                <Toggle
                  checked={form.third_party_apps ?? false}
                  onChange={(v) => set("third_party_apps", v)}
                  label={t("server.apps.thirdParty")}
                />
              </Row>
            </Group>
          </>
        );
      }}
    </SettingsForm>
  );
}

/** Shares of the processors other programs may keep busy before library work waits; 100: it never waits. */
const MAX_CPU = [25, 50, 75, 100];

/**
 * Requiring two-step sign-in: only an administrator who uses it can turn it
 * on, and turning it on signs out everyone without it, who sets it up at
 * their next sign-in.
 */
function RequireTwoStepRow({ checked, onChange, known }: { checked: boolean; onChange: (v: boolean) => void; known: boolean }) {
  const { t } = useTranslation("settings");
  const me = useAuth();
  const saved = useSettings().data?.require_two_step ?? false;
  const noKey = useSystemInfo().data?.secret_key_ok === false;
  const users = useUsers();
  const without = (users.data ?? []).filter((u) => u.has_password && !u.two_step && !u.disabled).length;
  // Off, it turns on only for someone who uses it; on, it can always go off.
  const blocked = !checked && !me.two_step;

  return (
    <Row label={t("server.signIn.requireTwoStep")} hint={t("server.signIn.requireTwoStepHint")}>
      {/* Off until a server that knows the setting sends it. */}
      <Toggle checked={checked} onChange={onChange} label={t("server.signIn.requireTwoStep")} disabled={!known || blocked} />
      {blocked ? (
        <Note>
          <Trans t={t} i18nKey="server.signIn.ownFirst" components={{ a: <RouterLink to="/account" className="text-accent hover:underline" /> }} />
        </Note>
      ) : checked && !saved && users.data ? (
        <Note notice>{without === 0 ? t("server.signIn.everyoneHasIt") : t("server.signIn.signsOut", { count: without })}</Note>
      ) : checked && saved && noKey ? (
        <Note notice>{t("server.signIn.noSecretKey")}</Note>
      ) : null}
    </Row>
  );
}

/** When intro and credits detection and previews for whole libraries run. */
function LibraryWorkRows({ form, set }: { form: Settings; set: <K extends keyof Settings>(key: K, value: Settings[K]) => void }) {
  const { t } = useTranslation("settings");
  const mode = form.library_work ?? "auto";
  const background = form.detect_markers || (form.seek_previews && form.previews_in_background);
  const week = useWatchingHours(form.library_work !== undefined && mode === "auto" && background);
  // A server before these settings.
  if (form.library_work === undefined) return null;
  const maxCpu = form.library_work_max_cpu ?? 50;
  const hours = Array.from({ length: 24 }, (_, h) => h);
  return (
    <>
      <Row label={t("server.libraryWork.label")} htmlFor="library-work" hint={t("server.libraryWork.hint")}>
        <Select
          id="library-work"
          value={mode}
          disabled={!background}
          onChange={(e) => set("library_work", e.target.value as LibraryWork)}
          className="w-full"
        >
          <option value="auto">{t("server.libraryWork.auto")}</option>
          <option value="hours">{t("server.libraryWork.hours")}</option>
          <option value="idle">{t("server.libraryWork.idle")}</option>
        </Select>
        {!background ? (
          <Note>{t("server.libraryWork.needs")}</Note>
        ) : mode === "auto" ? (
          <Note>{t("server.libraryWork.autoNote")}</Note>
        ) : mode === "hours" ? (
          <div className="mt-2 flex flex-wrap items-center gap-2 text-sm">
            <Select aria-label={t("server.libraryWork.from")} value={form.library_work_from ?? 1} onChange={(e) => set("library_work_from", Number(e.target.value))}>
              {hours.map((h) => (
                <option key={h} value={h}>
                  {hourOfDay(h)}
                </option>
              ))}
            </Select>
            <span className="text-muted">{t("server.libraryWork.between")}</span>
            <Select aria-label={t("server.libraryWork.to")} value={form.library_work_to ?? 7} onChange={(e) => set("library_work_to", Number(e.target.value))}>
              {hours.map((h) => (
                <option key={h} value={h}>
                  {hourOfDay(h)}
                </option>
              ))}
            </Select>
            <span className="text-xs text-subtle">{t("server.libraryWork.serverTime")}</span>
          </div>
        ) : null}
      </Row>
      {background && mode === "auto" && week.data ? (
        <div className="space-y-2">
          <div>
            <p className="text-sm font-medium">{t("server.libraryWork.whenPeopleWatch")}</p>
            <p className="mt-0.5 text-xs text-subtle">
              {week.data.learned ? t("server.libraryWork.learned") : t("server.libraryWork.learning")}
              {week.data.waiting === "hours" && week.data.resumes_at
                ? ` ${t("server.libraryWork.nextFrom", { time: formatTimestamp(week.data.resumes_at) })}`
                : ""}
            </p>
          </div>
          <WatchingHoursChart week={week.data} />
        </div>
      ) : null}
      <Row label={t("server.libraryWork.waitForOthers")} htmlFor="library-work-cpu" hint={t("server.libraryWork.waitForOthersHint")}>
        <Select
          id="library-work-cpu"
          value={maxCpu}
          disabled={!background}
          onChange={(e) => set("library_work_max_cpu", Number(e.target.value))}
          className="w-full"
        >
          {(MAX_CPU.includes(maxCpu) ? MAX_CPU : [...MAX_CPU, maxCpu].sort((a, b) => a - b)).map((v) => (
            <option key={v} value={v}>
              {v === 100 ? t("server.libraryWork.never") : t("server.libraryWork.over", { percent: v })}
            </option>
          ))}
        </Select>
      </Row>
    </>
  );
}
