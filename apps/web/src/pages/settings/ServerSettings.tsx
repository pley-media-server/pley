import { Link as RouterLink } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { appKeys } from "@/api/apps";
import { keys, useSettings, useSystemInfo, useUsers, useWatchingHours } from "@/api/hooks";
import type { LibraryWork, Settings, TvMetadata } from "@/api/types";
import { useAuth } from "@/auth";
import { Select, TextInput, Toggle } from "@/components/ui";
import { LanguageOptions } from "@/i18n/LanguageOptions";
import { useTranslation } from "react-i18next";
import { formatTimestamp, pluralize } from "@/lib/format";
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
  useDocumentTitle("Server settings");
  const { t } = useTranslation();
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
    <SettingsForm draft={draft} title="Server" onSaved={onSaved}>
      {(form) => {
        const hasTmdb = form.tmdb_api_key.trim() !== "";
        const hasTvdb = form.tvdb_api_key.trim() !== "";
        const noChromaprint = knownTranscoder(system.data)?.chromaprint === false;
        return (
          <>
            <Group title="General">
              <Row label="Server name" hint="Shown on the sign-in screen." htmlFor="server-name">
                <TextInput id="server-name" value={form.server_name} onChange={(e) => set("server_name", e.target.value)} />
              </Row>
              <Row label={t("language.label")} hint={t("language.serverHint")} htmlFor="server-language">
                <Select id="server-language" value={form.language} onChange={(e) => set("language", e.target.value)}>
                  <option value="">{t("language.eachDevice")}</option>
                  <LanguageOptions />
                </Select>
              </Row>
            </Group>

            <Group title="Metadata" description="Posters, summaries, ratings, cast and episode titles.">
              <Row
                label="TMDB API key"
                htmlFor="tmdb-key"
                hint={
                  <>
                    Movies, collections and Fix Match use TMDB. Free key from{" "}
                    <Link href="https://www.themoviedb.org/settings/api">themoviedb.org/settings/api</Link>. Without one, movies keep their file
                    names and local artwork.
                  </>
                }
              >
                <SecretInput id="tmdb-key" what="key" value={form.tmdb_api_key} onChange={(v) => setKey("tmdb_api_key", v)} />
                <TestButton key={form.tmdb_api_key} label="Test the TMDB key" test={{ service: "tmdb", api_key: form.tmdb_api_key }} />
              </Row>
              <Row
                label="TV shows"
                htmlFor="tv-source"
                hint={
                  <>
                    Automatic uses TMDB when it has a key, then TheTVDB when it has one, else <Link href="https://www.tvmaze.com">TVmaze</Link>,
                    which needs none. Changing the source fetches every show again.
                  </>
                }
              >
                <Select id="tv-source" value={form.tv_metadata} onChange={(e) => set("tv_metadata", e.target.value as TvMetadata)} className="w-full">
                  <option value="auto">Automatic ({TV_SOURCES[automaticTvSource(form)]})</option>
                  <option value="tmdb" disabled={!hasTmdb}>
                    TMDB{hasTmdb ? "" : " (needs an API key)"}
                  </option>
                  <option value="tvdb" disabled={!hasTvdb}>
                    TheTVDB{hasTvdb ? "" : " (needs an API key)"}
                  </option>
                  <option value="tvmaze">TVmaze (no key needed)</option>
                </Select>
                <TestButton label="Test TVmaze" test={{ service: "tvmaze" }} />
              </Row>
              <Row
                label="TheTVDB API key"
                htmlFor="tvdb-key"
                hint={
                  <>
                    Only needed to use TheTVDB for TV shows. Get one at <Link href="https://thetvdb.com/api-information">thetvdb.com/api-information</Link>.
                  </>
                }
              >
                <SecretInput id="tvdb-key" what="key" value={form.tvdb_api_key} onChange={(v) => setKey("tvdb_api_key", v)} />
              </Row>
              <Row label="TheTVDB PIN" htmlFor="tvdb-pin" hint="User-supported keys also need your subscriber PIN. Leave empty for other keys.">
                <SecretInput id="tvdb-pin" what="PIN" value={form.tvdb_pin} onChange={(v) => set("tvdb_pin", v)} />
                <TestButton
                  key={`${form.tvdb_api_key}
${form.tvdb_pin}`}
                  label="Test the TheTVDB key"
                  test={{ service: "tvdb", api_key: form.tvdb_api_key, pin: form.tvdb_pin }}
                />
              </Row>
              <Row
                label="Music metadata"
                hint={
                  <>
                    Genres and release dates from <Link href="https://musicbrainz.org">MusicBrainz</Link>, and covers from the Cover Art Archive for
                    albums that have none. Tags in the files win. No key needed; the first look-up takes about a second per album.
                  </>
                }
              >
                <Toggle checked={form.music_metadata} onChange={(v) => set("music_metadata", v)} label="Music metadata from MusicBrainz" />
                <TestButton label="Test MusicBrainz" test={{ service: "musicbrainz" }} />
              </Row>
              <Row label="Metadata language" htmlFor="meta-lang" hint="For TMDB and TheTVDB. TVmaze is in English.">
                <Select id="meta-lang" value={form.metadata_language} onChange={(e) => set("metadata_language", e.target.value)} className="w-full">
                  {(LANGUAGES.includes(form.metadata_language) ? LANGUAGES : [form.metadata_language, ...LANGUAGES]).map((l) => (
                    <option key={l} value={l}>
                      {l}
                    </option>
                  ))}
                </Select>
              </Row>
              <p className="text-xs text-subtle">
                This product uses the TMDB API but is not endorsed or certified by TMDB. TV metadata from{" "}
                <Link href="https://thetvdb.com">TheTVDB</Link>. TV data from TVmaze is licensed{" "}
                <Link href="https://creativecommons.org/licenses/by-sa/4.0/">CC BY-SA</Link>; music data from MusicBrainz is public domain.
              </p>
            </Group>

            <Group title="Subtitles" description="Find subtitles for a movie or episode on OpenSubtitles, from the player's subtitle menu.">
              <Row
                label="OpenSubtitles API key"
                htmlFor="opensubtitles-key"
                hint={
                  <>
                    Each server needs its own key: create a consumer at{" "}
                    <Link href="https://www.opensubtitles.com/consumers">opensubtitles.com/consumers</Link>. Without one, subtitle search is off.
                  </>
                }
              >
                <SecretInput
                  id="opensubtitles-key"
                  what="key"
                  value={form.opensubtitles_api_key ?? ""}
                  onChange={(v) => set("opensubtitles_api_key", v)}
                />
              </Row>
              <Row
                label="OpenSubtitles account"
                htmlFor="opensubtitles-username"
                hint="Optional. Downloads count against this account's daily quota instead of the key's smaller one."
              >
                <div className="grid gap-2 sm:grid-cols-2">
                  <TextInput
                    id="opensubtitles-username"
                    aria-label="OpenSubtitles username"
                    autoComplete="off"
                    spellCheck={false}
                    placeholder="Username"
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
                    aria-label="OpenSubtitles password"
                    type="password"
                    autoComplete="new-password"
                    placeholder={form.opensubtitles_password_set ? "Saved" : "Password"}
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
                  label="Test OpenSubtitles"
                  test={{
                    service: "opensubtitles",
                    api_key: form.opensubtitles_api_key ?? "",
                    username: form.opensubtitles_username ?? "",
                    ...(form.opensubtitles_password ? { password: form.opensubtitles_password } : {}),
                  }}
                />
              </Row>
            </Group>

            <Group title="Library scanning">
              <Row label="Watch folders for changes" hint="Pick up new, changed and deleted files automatically.">
                <Toggle checked={form.watch_libraries} onChange={(v) => set("watch_libraries", v)} label="Watch folders" />
              </Row>
              <Row
                label="Detect intros and credits"
                hint="Compare the audio of episodes in the background to offer Skip Intro and Skip Credits. Chapters named Intro or Credits are always used."
              >
                <Toggle
                  checked={form.detect_markers}
                  disabled={noChromaprint}
                  onChange={(v) => set("detect_markers", v)}
                  label="Detect intros and credits"
                />
                {noChromaprint ? <Note>This FFmpeg lacks Chromaprint, so only chapters can mark intros and credits.</Note> : null}
              </Row>
              <Row
                label="Seek previews"
                hint="Make the thumbnails shown above the seek bar, in the background, the first time a movie or episode is opened."
              >
                <Toggle checked={form.seek_previews} onChange={(v) => set("seek_previews", v)} label="Seek previews" />
              </Row>
              <Row
                label="Previews for whole libraries"
                hint="After each scan, make missing seek previews and chapter pictures."
              >
                <Toggle
                  checked={form.previews_in_background}
                  disabled={!form.seek_previews}
                  onChange={(v) => set("previews_in_background", v)}
                  label="Previews for whole libraries"
                />
                {form.seek_previews ? null : <Note>Needs Seek previews.</Note>}
              </Row>
              <LibraryWorkRows form={form} set={set} />
              <Row label="Periodic scan interval" htmlFor="scan-interval" hint="Minutes between full scans. 0 disables periodic scans.">
                <div className="flex items-center gap-2">
                  <TextInput
                    id="scan-interval"
                    type="number"
                    min={0}
                    value={form.scan_interval_minutes}
                    onChange={(e) => set("scan_interval_minutes", Math.max(0, Number(e.target.value) || 0))}
                    className="w-28"
                  />
                  <span className="text-sm text-muted">minutes</span>
                </div>
              </Row>
            </Group>

            <Group title="Sign-in">
              <RequireTwoStepRow
                checked={form.require_two_step ?? false}
                onChange={(v) => set("require_two_step", v)}
                known={form.require_two_step !== undefined}
              />
            </Group>

            <Group title="Apps">
              <Row
                label="Apps by other publishers"
                hint="Pley's own apps can always sign in. Turn this on to let people allow other publishers' apps; turning it off signs them out."
              >
                {/* Off until a server that knows the setting sends it. */}
                <Toggle
                  checked={form.third_party_apps ?? false}
                  onChange={(v) => set("third_party_apps", v)}
                  label="Apps by other publishers"
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
  const me = useAuth();
  const saved = useSettings().data?.require_two_step ?? false;
  const noKey = useSystemInfo().data?.secret_key_ok === false;
  const users = useUsers();
  const without = (users.data ?? []).filter((u) => u.has_password && !u.two_step && !u.disabled).length;
  // Off, it turns on only for someone who uses it; on, it can always go off.
  const blocked = !checked && !me.two_step;

  return (
    <Row
      label="Require two-step sign-in"
      hint="Everyone with a password signs in with a code from an authenticator app. Whoever hasn't set it up does so at their next sign-in, in the browser (for a TV, on the phone they sign in with)."
    >
      {/* Off until a server that knows the setting sends it. */}
      <Toggle checked={checked} onChange={onChange} label="Require two-step sign-in" disabled={!known || blocked} />
      {blocked ? (
        <Note>
          Turn on two-step sign-in for your own account first, in{" "}
          <RouterLink to="/account" className="text-accent hover:underline">
            Account
          </RouterLink>
          .
        </Note>
      ) : checked && !saved && users.data ? (
        <Note notice>
          {without === 0
            ? "Everyone with a password uses it already."
            : `Saving signs out ${pluralize(without, "person who hasn't", "people who haven't")} set it up, on every device and app, until they set it up.`}
        </Note>
      ) : checked && saved && noKey ? (
        <Note notice>
          The server can't use its secret.key, so authenticator apps can't be set up or checked. Until it can, people who haven't set up two-step sign-in can't sign in, and people who have sign in with a recovery code. Its log says why.
        </Note>
      ) : null}
    </Row>
  );
}

/** When intro and credits detection and previews for whole libraries run. */
function LibraryWorkRows({ form, set }: { form: Settings; set: <K extends keyof Settings>(key: K, value: Settings[K]) => void }) {
  const mode = form.library_work ?? "auto";
  const background = form.detect_markers || (form.seek_previews && form.previews_in_background);
  const week = useWatchingHours(form.library_work !== undefined && mode === "auto" && background);
  // A server before these settings.
  if (form.library_work === undefined) return null;
  const maxCpu = form.library_work_max_cpu ?? 50;
  const hours = Array.from({ length: 24 }, (_, h) => h);
  return (
    <>
      <Row
        label="When library work runs"
        htmlFor="library-work"
        hint="Finding intros and credits, and previews for whole libraries. They always wait while something plays; what someone opens or is watching goes first, at any hour."
      >
        <Select
          id="library-work"
          value={mode}
          disabled={!background}
          onChange={(e) => set("library_work", e.target.value as LibraryWork)}
          className="w-full"
        >
          <option value="auto">Automatically</option>
          <option value="hours">Between set hours</option>
          <option value="idle">Whenever nothing plays</option>
        </Select>
        {!background ? (
          <Note>Needs Detect intros and credits, or Previews for whole libraries.</Note>
        ) : mode === "auto" ? (
          <Note>Shows people are watching, titles on their lists and what's new go first, at any hour. The rest waits for the hours people rarely watch, which the server learns.</Note>
        ) : mode === "hours" ? (
          <div className="mt-2 flex flex-wrap items-center gap-2 text-sm">
            <Select aria-label="From" value={form.library_work_from ?? 1} onChange={(e) => set("library_work_from", Number(e.target.value))}>
              {hours.map((h) => (
                <option key={h} value={h}>
                  {hourOfDay(h)}
                </option>
              ))}
            </Select>
            <span className="text-muted">to</span>
            <Select aria-label="To" value={form.library_work_to ?? 7} onChange={(e) => set("library_work_to", Number(e.target.value))}>
              {hours.map((h) => (
                <option key={h} value={h}>
                  {hourOfDay(h)}
                </option>
              ))}
            </Select>
            <span className="text-xs text-subtle">in the server's time</span>
          </div>
        ) : null}
      </Row>
      {background && mode === "auto" && week.data ? (
        <div className="space-y-2">
          <div>
            <p className="text-sm font-medium">When people watch</p>
            <p className="mt-0.5 text-xs text-subtle">
              {week.data.learned
                ? "Learned from what played over the last weeks, recent ones counting most, in the server's time."
                : "Still learning: until about a week of playback is known, the rest of the libraries goes on whenever nothing plays."}
              {week.data.waiting === "hours" && week.data.resumes_at ? ` Next from ${formatTimestamp(week.data.resumes_at)}.` : ""}
            </p>
          </div>
          <WatchingHoursChart week={week.data} />
        </div>
      ) : null}
      <Row label="Wait for other programs" htmlFor="library-work-cpu" hint="Library work waits while other programs keep more of the processors busy than this.">
        <Select
          id="library-work-cpu"
          value={maxCpu}
          disabled={!background}
          onChange={(e) => set("library_work_max_cpu", Number(e.target.value))}
          className="w-full"
        >
          {(MAX_CPU.includes(maxCpu) ? MAX_CPU : [...MAX_CPU, maxCpu].sort((a, b) => a - b)).map((v) => (
            <option key={v} value={v}>
              {v === 100 ? "Never" : `When they use over ${v}%`}
            </option>
          ))}
        </Select>
      </Row>
    </>
  );
}
