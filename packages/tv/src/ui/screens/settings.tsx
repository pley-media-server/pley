import type { ComponentChildren } from "preact";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import { errorMessage, isApiError } from "../../api/client";
import type { HistoryEntry, SubtitleMode } from "../../api/types";
import { displayAddress } from "../../auth/addresses";
import { episodeLabel, relativeTime } from "../../format/format";
import { deviceLanguage, LANGUAGES as APP_LANGUAGES, supportedLanguage, t, type Language } from "../../i18n";
import { AUTO, ORIGINAL, PRESETS, presetById } from "../../player/quality";
import type { AppState, Settings } from "../../state";
import { fontsHere, SUBTITLE_FONTS, SUBTITLE_POSITIONS } from "../../subs/style";
import { useUi } from "../context";
import { Art, artCandidates } from "../kit/art";
import { Pill, TextButton } from "../kit/buttons";
import { F, G } from "../kit/focusable";
import { Icon, type IconName } from "../kit/icons";
import { PanelRow } from "../kit/buttons";
import { PanelTitle, SidePanel, useScrollToFocus, useTrap } from "../kit/panel";
import type { Entry } from "../shell/navigator";
import { Dialog, failedTitle, problem, sentence, useApp } from "./common";
import { licenseRows } from "../../licenses";
import { pageOf } from "./item-actions";
import { librarySorting, SORT_NEEDS_NEWER } from "./library";
import { ProfileAvatar, QrCode } from "./onboarding/kit";
import { TMDB_NOTICE } from "../../requests";

type Category = "profile" | "playback" | "audio" | "display" | "server" | "diagnostics" | "about" | "signout";

const CATEGORIES: { id: Category; label: string; icon: IconName }[] = [
  { id: "profile", label: "Profile", icon: "user-round" },
  { id: "playback", label: "Playback", icon: "play" },
  { id: "audio", label: "Audio", icon: "audio-lines" },
  { id: "display", label: "Subtitles and display", icon: "captions" },
  { id: "server", label: "Server", icon: "server" },
  { id: "diagnostics", label: "Diagnostics", icon: "gauge" },
  { id: "about", label: "About", icon: "info" },
  { id: "signout", label: "Sign out", icon: "log-out" },
];

/** The languages offered for track defaults, as ISO 639-2/B codes (the Android app's 34). */
export const LANGUAGES: [string, string][] = [
  ["ara", "Arabic"], ["bul", "Bulgarian"], ["cat", "Catalan"], ["chi", "Chinese"], ["hrv", "Croatian"], ["cze", "Czech"], ["dan", "Danish"],
  ["dut", "Dutch"], ["eng", "English"], ["fin", "Finnish"], ["fre", "French"], ["ger", "German"], ["gre", "Greek"], ["heb", "Hebrew"],
  ["hin", "Hindi"], ["hun", "Hungarian"], ["ind", "Indonesian"], ["ita", "Italian"], ["jpn", "Japanese"], ["kor", "Korean"], ["nor", "Norwegian"],
  ["per", "Persian"], ["pol", "Polish"], ["por", "Portuguese"], ["rum", "Romanian"], ["rus", "Russian"], ["srp", "Serbian"], ["slo", "Slovak"],
  ["spa", "Spanish"], ["swe", "Swedish"], ["tha", "Thai"], ["tur", "Turkish"], ["ukr", "Ukrainian"], ["vie", "Vietnamese"],
];

const SUBTITLE_MODES: { id: SubtitleMode; label: string; hint: string }[] = [
  { id: "off", label: "Off", hint: "Subtitles stay off until you turn them on." },
  { id: "forced", label: "Only for foreign dialogue", hint: "Only forced subtitles, for signs and foreign dialogue." },
  { id: "foreign", label: "When the audio is in another language", hint: "Full subtitles come on when the audio isn't in your subtitle language." },
  { id: "always", label: "Always", hint: "Subtitles in your language come on whenever there are some." },
];

const SIZES: { id: Settings["subtitleSize"]; label: string }[] = [
  { id: "small", label: "Small" },
  { id: "normal", label: "Medium" },
  { id: "large", label: "Large" },
  { id: "largest", label: "Largest" },
];


interface Choice<T> {
  value: T;
  label: string;
  hint?: string;
}

type OpenPanel = { title: string; choices: Choice<unknown>[]; current: unknown; pick: (v: unknown) => void } | null;

/**
 * Settings (TvSettingsScreen): the categories at the left; the shown one's
 * rows at the right. Track defaults are the profile's on the server; the
 * rest are this TV's.
 */
export function SettingsScreen(p: { entry: Entry }) {
  const app = useApp();
  const ui = useUi();
  const settings = app.settings.value;
  const me = app.me.value;
  const [shown, setShown] = useState<Category>((p.entry.state.category as Category | undefined) ?? "profile");
  const [panel, setPanel] = useState<OpenPanel>(null);
  const [panelFrom, setPanelFrom] = useState<string | null>(null);
  const [signOut, setSignOut] = useState(false);
  const [web, setWeb] = useState(false);
  const [readOnly, setReadOnly] = useState(false);
  const [signingOut, setSigningOut] = useState(false);
  const paneRef = useRef<HTMLDivElement>(null);
  const shift = useScrollToFocus(paneRef, 540 - 81 - 27);
  const scopes = app.tokenStore?.scopes() ?? [];
  const canSwitch = scopes.indexOf("profiles") >= 0;
  const canDefaults = scopes.indexOf("play") >= 0 && !readOnly;
  const librariesSort = librarySorting(app);
  const server = app.server.value;
  const household = app.profiles.value ?? app.savedHousehold();
  const show = (c: Category) => {
    setShown(c);
    p.entry.state.category = c;
  };
  const focusRow = (fid: string) =>
    setTimeout(() => {
      const el = document.querySelector<HTMLElement>(`[data-fid="${fid}"]`);
      if (el) ui.focus.focus(el);
    }, 0);
  const intoPane = () => {
    const first = paneRef.current?.querySelector<HTMLElement>("[data-f]:not([data-fdisabled])");
    return !!first && ui.focus.focus(first);
  };
  const choose = <T,>(fid: string, title: string, choices: Choice<T>[], current: T, pick: (v: T) => void) => {
    setPanelFrom(fid);
    setPanel({ title, choices: choices as Choice<unknown>[], current, pick: pick as (v: unknown) => void });
  };
  const closePanel = () => {
    setPanel(null);
    if (panelFrom) focusRow(panelFrom);
  };

  const saveDefaults = (patch: { audio?: string | null; subtitle?: string | null; mode?: SubtitleMode }) => {
    if (!me) return;
    const audio = patch.audio !== undefined ? patch.audio : me.audio_language;
    const subtitle = patch.subtitle !== undefined ? patch.subtitle : me.subtitle_language;
    const mode = patch.mode ?? me.subtitle_mode;
    if (audio === me.audio_language && subtitle === me.subtitle_language && mode === me.subtitle_mode) return;
    const before = me;
    app.me.value = { ...me, audio_language: audio, subtitle_language: subtitle, subtitle_mode: mode };
    ui.api.setTrackDefaults(audio, subtitle, mode).then(
      () => {
        // Titles kept start with the tracks these pick now.
        app.tracksChanged(null);
        ui.toast("Saved");
      },
      (e) => {
        app.me.value = before;
        if (isApiError(e, 403) || isApiError(e, 404) || isApiError(e, 405)) {
          setReadOnly(true);
          ui.toast("This server doesn't let the app change these: change them on the web", true);
        } else ui.toast(sentence(errorMessage(e)), true);
      },
    );
  };
  const setting = (patch: Partial<Settings>) => app.updateSettings(patch);
  const langName = (code: string | null) => (code ? LANGUAGES.filter((l) => l[0] === code)[0]?.[1] ?? code : null);
  const webSuffix = canDefaults ? "" : " · change on the web";

  // The language Pley is in for the profile: the server's until it picks one, each language named in itself.
  const shownIn = supportedLanguage(me?.language) ?? null;
  const serverLanguage = supportedLanguage(app.serverLanguage.value);
  const serverDefault = t("settings.language.serverDefault", {
    language: serverLanguage
      ? t(`settings.language.names.${serverLanguage}`)
      : `${t("settings.language.deviceLanguage")}, ${t(`settings.language.names.${deviceLanguage(app.deviceLanguages()) ?? "en"}`)}`,
  });
  const languageChoices: Choice<Language | null>[] = [
    { value: null, label: serverDefault },
    ...APP_LANGUAGES.map((l) => ({ value: l, label: t(`settings.language.names.${l}`) })),
  ];
  const saveLanguage = (next: Language | null) => {
    if (!me || next === shownIn) return;
    app.setAccountLanguage(next).catch((e: unknown) => {
      ui.toast(isApiError(e, 404) || isApiError(e, 405) ? t("settings.language.needsNewer") : sentence(errorMessage(e)), true);
    });
  };

  const rows: ComponentChildren[] = [];
  switch (shown) {
    case "profile":
      rows.push(
        <div key="header" class="settings-profile">
          {me ? (
            <>
              <ProfileAvatar name={me.display_name || me.name} color={me.avatar_color} picture={me.avatar_url} size={48} />
              <div style="margin-left:16px;min-width:0">
                <div class="t-title c-fg one-line">{me.display_name || me.name}</div>
                {/* A managed profile says so; an account needs no word for what it is (Android's header). */}
                <div class="t-caption c-muted one-line">{[me.managed ? "Managed profile" : "", server ? displayAddress(server) : ""].filter(Boolean).join(" · ")}</div>
              </div>
            </>
          ) : (
            <div style="height:60px" />
          )}
        </div>,
      );
      if (canSwitch) rows.push(<SettingRow key="switch" fid="row-switch" icon="users" label="Switch profile" chevron onSelect={() => ui.nav.restart({ name: "profiles" })} />);
      rows.push(
        <SettingRow
          key="language"
          fid="row-language"
          icon="languages"
          label={t("settings.language.label")}
          value={me ? languageChoices.filter((c) => c.value === shownIn)[0]?.label : undefined}
          onSelect={() => choose("row-language", t("settings.language.label"), languageChoices, shownIn, saveLanguage)}
        />,
      );
      rows.push(
        <SettingRow
          key="audio"
          fid="row-audio"
          icon="audio-lines"
          label="Audio language"
          value={me ? `${langName(me.audio_language) ?? "The file's default"}${webSuffix}` : undefined}
          onSelect={() =>
            canDefaults
              ? choose("row-audio", "Audio language", [{ value: null, label: "The file's default" }, ...LANGUAGES.map(([v, l]) => ({ value: v, label: l }))], me?.audio_language ?? null, (v) =>
                  saveDefaults({ audio: v as string | null }),
                )
              : setWeb(true)
          }
        />,
      );
      rows.push(
        <SettingRow
          key="subs"
          fid="row-subtitles"
          icon="captions"
          label="Subtitles"
          value={me ? `${SUBTITLE_MODES.filter((m) => m.id === me.subtitle_mode)[0]?.label ?? ""}${webSuffix}` : undefined}
          onSelect={() =>
            canDefaults
              ? choose(
                  "row-subtitles",
                  "Subtitles",
                  SUBTITLE_MODES.map((m) => ({ value: m.id, label: m.label, hint: m.hint })),
                  me?.subtitle_mode ?? "forced",
                  (v) => saveDefaults({ mode: v as SubtitleMode }),
                )
              : setWeb(true)
          }
        />,
      );
      if (me && (me.subtitle_mode === "foreign" || me.subtitle_mode === "always")) {
        rows.push(
          <SettingRow
            key="sublang"
            fid="row-sublang"
            icon="captions"
            label="Subtitle language"
            value={`${langName(me.subtitle_language) ?? "Same as the audio language"}${webSuffix}`}
            onSelect={() =>
              canDefaults
                ? choose(
                    "row-sublang",
                    "Subtitle language",
                    [{ value: null, label: "Same as the audio language" }, ...LANGUAGES.map(([v, l]) => ({ value: v, label: l }))],
                    me.subtitle_language,
                    (v) => saveDefaults({ subtitle: v as string | null }),
                  )
                : setWeb(true)
            }
          />,
        );
      }
      rows.push(<SettingRow key="history" fid="row-history" icon="history" label="Watch history" chevron onSelect={() => ui.nav.open({ name: "history" })} />);
      // The Libraries page sorts them too, but with six or fewer the rail lists them and has no way there.
      if (librariesSort !== "no") {
        rows.push(
          <SettingRow
            key="sortLibraries"
            fid="row-sort-libraries"
            icon="arrow-up-down"
            label="Sort libraries"
            value={librariesSort === "older" ? "Needs a newer Pley on the server" : `Your order, on all your devices${librariesSort === "refused" ? " · change on the web" : ""}`}
            chevron={librariesSort === "yes"}
            onSelect={() =>
              librariesSort === "yes" ? ui.nav.open({ name: "libraries", sort: true }) : librariesSort === "refused" ? setWeb(true) : ui.toast(SORT_NEEDS_NEWER, true)
            }
          />,
        );
      }
      if (household.length > 1) {
        const always = app.alwaysStartAs();
        rows.push(
          <SettingRow
            key="always"
            fid="row-always"
            icon="user-round"
            label="Always start as"
            value={always === null ? "Ask who's watching" : household.filter((h) => h.id === always)[0]?.name ?? "Ask who's watching"}
            onSelect={() =>
              choose("row-always", "Always start as", [{ value: null, label: "Ask who's watching" }, ...household.map((h) => ({ value: h.id, label: h.name }))], always, (v) =>
                app.setAlwaysStartAs(v as number | null),
              )
            }
          />,
        );
      }
      rows.push(
        <SettingRow key="web" fid="row-web" icon="monitor" label="Manage your account on the web" value="Name, picture, password, PIN, two-step, devices and apps" onSelect={() => setWeb(true)} />,
      );
      break;
    case "playback":
      rows.push(
        <SettingRow
          key="quality"
          fid="row-quality"
          icon="gauge"
          label="Quality"
          value={qualityName(settings.quality)}
          onSelect={() =>
            choose(
              "row-quality",
              "Quality",
              PRESETS.map((q) => ({ value: q.id, label: q.label, hint: qualityHint(q.id) })),
              settings.quality,
              (v) => setting({ quality: v as string }),
            )
          }
        />,
      );
      rows.push(
        <SettingRow key="autoplay" fid="row-autoplay" icon="skip-forward" label="Play the next episode by itself" value="After a 10 second countdown" toggle={settings.autoplayNext} onSelect={() => setting({ autoplayNext: !settings.autoplayNext })} />,
      );
      rows.push(
        <SettingRow
          key="details"
          fid="row-details"
          icon="info"
          label="Playback details"
          value="Show how a video plays (direct, remux or transcode) over it"
          toggle={settings.playbackDetails}
          onSelect={() => setting({ playbackDetails: !settings.playbackDetails })}
        />,
      );
      break;
    case "audio":
      rows.push(
        <div key="intro" class="settings-intro t-body c-muted">
          Dolby Digital, Dolby Digital Plus and Atmos are always passed through. Turn these on if your receiver supports them; otherwise Pley converts them.
        </div>,
      );
      rows.push(
        <SettingRow
          key="truehd"
          fid="row-truehd"
          icon="audio-lines"
          label="Pass through Dolby TrueHD"
          value="Only with a receiver that takes TrueHD from the TV"
          toggle={settings.passthroughTrueHd}
          onSelect={() => setting({ passthroughTrueHd: !settings.passthroughTrueHd })}
        />,
      );
      rows.push(
        <SettingRow
          key="dts"
          fid="row-dts"
          icon="audio-lines"
          label="Pass through DTS"
          value="Only with a receiver that takes DTS from the TV"
          toggle={settings.passthroughDts}
          onSelect={() => setting({ passthroughDts: !settings.passthroughDts })}
        />,
      );
      break;
    case "display":
      rows.push(
        <SettingRow
          key="size"
          fid="row-size"
          icon="captions"
          label="Subtitle size"
          value={SIZES.filter((s) => s.id === settings.subtitleSize)[0]?.label ?? "Medium"}
          onSelect={() =>
            choose("row-size", "Subtitle size", SIZES.map((s) => ({ value: s.id, label: s.label })), settings.subtitleSize, (v) => setting({ subtitleSize: v as Settings["subtitleSize"] }))
          }
        />,
      );
      rows.push(
        <SettingRow
          key="box"
          fid="row-box"
          icon="captions"
          label="Subtitle background"
          value={settings.subtitleBox ? "Dark box" : "None, with an outline"}
          onSelect={() =>
            choose(
              "row-box",
              "Subtitle background",
              [
                { value: false, label: "None, with an outline" },
                { value: true, label: "Dark box" },
              ],
              settings.subtitleBox,
              (v) => setting({ subtitleBox: v as boolean }),
            )
          }
        />,
      );
      rows.push(
        <SettingRow
          key="font"
          fid="row-font"
          icon="captions"
          label="Subtitle font"
          value={SUBTITLE_FONTS.filter((f) => f.id === settings.subtitleFont)[0]?.label ?? "Default"}
          onSelect={() =>
            choose(
              "row-font",
              "Subtitle font",
              SUBTITLE_FONTS.filter((f) => f.id === settings.subtitleFont || fontsHere().indexOf(f.id) >= 0).map((f) => ({ value: f.id, label: f.label })),
              settings.subtitleFont,
              (v) => setting({ subtitleFont: v as Settings["subtitleFont"] }),
            )
          }
        />,
      );
      rows.push(
        <SettingRow
          key="position"
          fid="row-position"
          icon="captions"
          label="Subtitle position"
          value={SUBTITLE_POSITIONS.filter((p) => p.id === settings.subtitlePosition)[0]?.label ?? "Bottom"}
          onSelect={() =>
            choose(
              "row-position",
              "Subtitle position",
              SUBTITLE_POSITIONS.map((p) => ({ value: p.id, label: p.label })),
              settings.subtitlePosition,
              (v) => setting({ subtitlePosition: v as Settings["subtitlePosition"] }),
            )
          }
        />,
      );
      rows.push(
        <SettingRow
          key="motion"
          fid="row-motion"
          icon="layers"
          label="Reduce motion"
          value="No zooming, sliding or fading: things change in place"
          toggle={settings.reduceMotion}
          onSelect={() => setting({ reduceMotion: !settings.reduceMotion })}
        />,
      );
      break;
    case "server":
      rows.push(<SettingRow key="server" fid="row-server" icon="server" label={server ? displayAddress(server) : "No server"} value="The server this TV uses" onSelect={() => undefined} />);
      rows.push(<SettingRow key="switchserver" fid="row-switchserver" icon="arrow-left" label="Switch server" chevron onSelect={() => ui.nav.restart({ name: "servers" })} />);
      break;
    case "diagnostics": {
      rows.push(
        <SettingRow
          key="diagnostics"
          fid="row-diagnostics"
          icon="gauge"
          label="Open diagnostics"
          value="What this TV decodes and shows, and why"
          chevron
          onSelect={() => ui.nav.open({ name: "diagnostics" })}
        />,
      );
      rows.push(
        <div key="facts" class="settings-facts">
          {deviceFacts(app).map(([l, v]) => (
            <div key={l} class="settings-fact">
              <div class="t-caption c-muted">{l}</div>
              <div class="t-caption c-fg">{v}</div>
            </div>
          ))}
        </div>,
      );
      break;
    }
    case "about":
      rows.push(<SettingRow key="version" fid="row-version" icon="info" label="Version" value={`${app.platform.appName} ${buildLabel(app.platform.version)}`} onSelect={() => undefined} />);
      rows.push(
        <div key="lic" class="settings-heading t-label c-subtle">
          Open-source licences
        </div>,
      );
      // Their texts are in the app's package, in NOTICES.txt.
      for (const [name, licence] of licenseRows()) rows.push(<SettingRow key={name} fid={`row-lic-${name}`} label={name} value={licence} onSelect={() => undefined} />);
      rows.push(
        <div key="credits" class="settings-heading t-label c-subtle">
          Credits
        </div>,
      );
      // Discover's titles and pictures (TMDB's terms ask for this line).
      rows.push(<SettingRow key="tmdb" fid="row-tmdb" label="TMDB" value={TMDB_NOTICE} onSelect={() => undefined} />);
      break;
    case "signout":
      rows.push(
        <div key="intro" class="settings-intro t-body c-muted">
          {`Sign this TV out of ${server ? displayAddress(server) : "the server"}. You'll need a new link code to sign in again.`}
        </div>,
      );
      rows.push(<SettingRow key="signout" fid="row-signout" icon="log-out" label="Sign out" danger disabled={signingOut} onSelect={() => setSignOut(true)} />);
      break;
  }

  return (
    <div class="settings-page">
      <div class="t-page c-fg" style="position:absolute;left:96px;top:27px">
        Settings
      </div>
      <G kind="col" restore class="settings-categories">
        {CATEGORIES.map((c) => (
          <SettingRow
            key={c.id}
            fid={`category-${c.id}`}
            icon={c.icon}
            label={c.label}
            danger={c.id === "signout"}
            chevron={shown === c.id}
            initial={c.id === shown}
            onFocus={() => show(c.id)}
            onSelect={() => (c.id === "signout" ? setSignOut(true) : intoPane())}
            onKey={(key) => (key === "right" ? intoPane() : false)}
          />
        ))}
      </G>
      <div class="settings-pane-clip">
        <div ref={paneRef} class="settings-pane" style={`transform:translateY(${-shift}px)`}>
          <G kind="col">{rows}</G>
        </div>
      </div>
      {panel && (
        <SidePanel onClose={closePanel} label={panel.title}>
          <PanelTitle>{panel.title}</PanelTitle>
          {panel.choices.map((c, i) => (
            <PanelRow
              key={i}
              label={c.label}
              sub={c.hint}
              checked={c.value === panel.current}
              initial={c.value === panel.current}
              onSelect={() => {
                panel.pick(c.value);
                closePanel();
              }}
            />
          ))}
        </SidePanel>
      )}
      {signOut && (
        <SignOutDialog
          server={server ? displayAddress(server) : null}
          onCancel={() => {
            setSignOut(false);
            focusRow("category-signout");
          }}
          onSignOut={() => {
            setSignOut(false);
            setSigningOut(true);
            void app.signOut().then(() => {
              if (server) ui.nav.restart({ name: "link", server });
              else ui.nav.restart({ name: "servers" });
            });
          }}
        />
      )}
      {web && server && (
        <WebDialog
          url={`${server}/account`}
          onClose={() => {
            setWeb(false);
            focusRow("row-web");
          }}
        />
      )}
    </div>
  );
}

/** "Automatic", "Original", "30 Mbps 4K": as the player's Quality menu says. */
function qualityName(id: string): string {
  return (presetById(id) ?? AUTO).label;
}

/** What the original and Automatic do, under their names in the list. */
function qualityHint(id: string): string | undefined {
  if (id === AUTO.id) return "Dynamically adjust to conditions";
  if (id === ORIGINAL.id) return "Full quality. May buffer on a slow connection";
  return undefined;
}

/** A row of Settings (TvSettingRow): an icon, a label, a value line; a switch or a chevron at the end. */
function SettingRow(p: {
  fid: string;
  icon?: IconName;
  label: string;
  value?: string;
  chevron?: boolean;
  toggle?: boolean;
  danger?: boolean;
  disabled?: boolean;
  initial?: boolean;
  onSelect: () => void;
  onFocus?: () => void;
  onKey?: (key: string) => boolean;
}) {
  return (
    <F
      class={`setting-row${p.danger ? " danger" : ""}`}
      fid={p.fid}
      initial={p.initial}
      disabled={p.disabled}
      label={p.value ? `${p.label}, ${p.value}` : p.label}
      onSelect={p.onSelect}
      onFocus={p.onFocus}
      onKey={p.onKey ? (k) => p.onKey!(k) : undefined}
    >
      {p.icon && <Icon name={p.icon} size={20} class="lead" />}
      <div style="flex:1;min-width:0">
        <div class="t-body one-line row-label">{p.label}</div>
        {p.value && <div class="t-caption lines-2 row-value">{p.value}</div>}
      </div>
      {p.toggle !== undefined && (
        <div class={`switch${p.toggle ? " on" : ""}`}>
          <div />
        </div>
      )}
      {p.chevron && <Icon name="chevron-right" size={20} class="chev" />}
    </F>
  );
}

/** Manage your account on the web (TvWebDialog): the address, and a QR code of it. */
function WebDialog(p: { url: string; onClose: () => void }) {
  const ui = useUi();
  const trap = useTrap();
  useEffect(
    () =>
      ui.input.push((key, e) => {
        if (key !== "back") return false;
        if (e.type === "down") p.onClose();
        return true;
      }),
    [],
  );
  return (
    <div ref={trap} class="dialog-root">
      <div class="scrim in" onClick={p.onClose} />
      <div class="dialog web-dialog">
        <div style="width:300px">
          <div class="t-title c-fg">Manage your account on the web</div>
          <div class="t-body c-muted" style="margin-top:10px">
            Scan the code with your phone, or go to this address on a phone or computer on the same network:
          </div>
          <div class="t-title" style="margin-top:10px;color:var(--accent-bright)">
            {p.url.replace(/^https?:\/\//, "")}
          </div>
          <G kind="row" style="margin-top:18px">
            <Pill label="Close" initial onSelect={p.onClose} />
          </G>
        </div>
        <div class="web-qr">
          <QrCode text={p.url} size={212} />
        </div>
      </div>
    </div>
  );
}

// ---- Watch history

/** "3 hours ago" and the like, which History was the first to write. */
export { relativeTime };

/** Watch history (TvHistoryScreen): newest first, 15 at a time, each removable. */
export function HistoryScreen(p: { entry: Entry }) {
  const ui = useUi();
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null);
  const [total, setTotal] = useState(0);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const shift = useScrollToFocus(listRef, 540 - 81);
  const load = (offset: number) => {
    setLoading(true);
    ui.api.history(offset, 15).then(
      (r) => {
        setLoading(false);
        setTotal(r.total);
        setEntries((old) => {
          const have = new Set((offset ? old ?? [] : []).map((e) => e.id));
          return (offset ? old ?? [] : []).concat(r.entries.filter((e) => !have.has(e.id)));
        });
      },
      (e) => {
        setLoading(false);
        setError(e);
      },
    );
  };
  useEffect(() => load(0), []);
  const remove = (e: HistoryEntry) => {
    const before = entries;
    setEntries((list) => (list ?? []).filter((x) => x.id !== e.id));
    setTotal((t) => t - 1);
    ui.api.removeHistory(e.id).catch((err) => {
      setEntries(before);
      setTotal((t) => t + 1);
      setError(err);
    });
  };
  void p;
  if (!entries) {
    if (error) {
      return (
        <HistoryMessage title={failedTitle(error, "Couldn't load your watch history")} body={problem(error)} action="Try again" onAction={() => (setError(null), load(0))} />
      );
    }
    return (
      <div class="history-page" role="status" aria-label="Loading">
        {[0, 1, 2, 3, 4].map((i) => (
          <div key={i} class="skeleton" style={`position:absolute;left:96px;top:${81 + i * 64}px;width:560px;height:52px;border-radius:4px`} />
        ))}
      </div>
    );
  }
  if (!entries.length) return <HistoryMessage title="No history yet" body="Movies, episodes and songs you finish show up here." action="Back" onAction={() => ui.nav.pop()} />;
  return (
    <div class="history-page">
      <div class="history-header">
        <div class="t-page c-fg">Watch history</div>
        <div class="t-caption c-muted" style="margin-left:16px;padding-bottom:6px">
          {total === 1 ? "1 play" : `${total} plays`}
        </div>
        <div style="flex:1" />
        {error ? <div class="t-label c-danger one-line">{sentence(errorMessage(error))}</div> : null}
      </div>
      <div class="history-viewport">
        <div ref={listRef} class="scroll-y" style={`transform:translateY(${-shift}px)`}>
          <G kind="col">
            {entries.map((e, i) => (
              <G key={e.id} kind="row" class="history-line">
                <F
                  class="history-row"
                  fid={`history/${e.id}`}
                  initial={i === 0}
                  label={historyTitle(e)}
                  onSelect={() => ui.nav.open(pageOf(e.item))}
                  onLongSelect={() => remove(e)}
                >
                  <div class={`history-art${e.item.kind === "track" || e.item.kind === "album" || e.item.kind === "artist" ? " square" : ""}`}>
                    <Art candidates={artCandidates(e.item, "poster")} width={48} title={e.item.title} class="tile-clip" />
                  </div>
                  <div style="flex:1;min-width:0;margin-left:16px">
                    <div class="t-body one-line">{historyTitle(e)}</div>
                    <div class="t-caption one-line sub">{[historySubtitle(e), relativeTime(e.viewed_at), e.device ?? ""].filter(Boolean).join(" · ")}</div>
                  </div>
                </F>
                <F class="history-remove" label={`Remove ${historyTitle(e)} from history`} onSelect={() => remove(e)}>
                  <Icon name="x" size={20} />
                </F>
              </G>
            ))}
            {entries.length < total && (
              <div style="margin-top:12px">
                <Pill label={loading ? "Loading…" : "Show more"} icon="chevron-down" disabled={loading} onSelect={() => load(entries.length)} />
              </div>
            )}
          </G>
        </div>
      </div>
    </div>
  );
}

function historyTitle(e: HistoryEntry): string {
  return e.item.kind === "episode" ? e.item.grandparent_title || e.item.title : e.item.title;
}

function historySubtitle(e: HistoryEntry): string {
  const it = e.item;
  if (it.kind === "episode") return [episodeLabel(it.parent_index, it.index), it.title].filter(Boolean).join(" · ");
  if (it.kind === "track") return [it.grandparent_title ?? "", it.parent_title ?? ""].filter(Boolean).join(" · ");
  return it.year ? String(it.year) : "";
}

function HistoryMessage(p: { title: string; body: string; action: string; onAction: () => void }) {
  return (
    <div class="history-page">
      <div style="position:absolute;left:96px;top:60px;width:520px">
        <div class="t-title c-fg">{p.title}</div>
        <div class="t-body c-muted" style="margin-top:10px">
          {p.body}
        </div>
        <G kind="row" style="margin-top:22px">
          <TextButton label={p.action} initial onSelect={p.onAction} />
        </G>
      </div>
    </div>
  );
}

/** What the TV says about itself, label and value (Settings' Diagnostics and the Diagnostics page). */
function deviceFacts(app: AppState): [string, string][] {
  const d = app.platform.device();
  return [
    ["Model", d.model],
    ["System", d.os],
    ["Firmware", d.firmware ?? "—"],
    ["Web engine", d.chromium ? `Chromium ${d.chromium}` : "—"],
    ["Tier", app.platform.tier()],
  ];
}

/**
 * Diagnostics (TvDiagnosticsScreen): the app and the TV, then what the
 * TV told the server it plays, one line each. Every line takes focus, so
 * the remote scrolls through them; nothing acts but Check again, after a
 * check that failed: it keeps focus while it checks, and a check that
 * works hands focus to the first line it found.
 */
export function DiagnosticsScreen() {
  const app = useApp();
  const ui = useUi();
  const [lines, setLines] = useState<string[] | null | "failed">(null);
  const [checking, setChecking] = useState(true);
  const again = useRef(false);
  const check = (fresh: boolean) => {
    setChecking(true);
    again.current = fresh;
    void app.caps(fresh).then((r) => {
      setLines(r ? r.lines : "failed");
      setChecking(false);
    });
  };
  useEffect(() => check(false), []);
  const column = useRef<HTMLDivElement>(null);
  const shift = useScrollToFocus(column, 540 - 27);
  useLayoutEffect(() => {
    if (!again.current || !Array.isArray(lines)) return;
    again.current = false;
    const first = column.current?.querySelector<HTMLElement>('[data-fid="plays/0"]');
    if (first && !ui.focus.current?.isConnected) ui.focus.focus(first);
  }, [lines]);
  const facts: [string, string][] = [["App", `${app.platform.appName} ${buildLabel(app.platform.version)}`] as [string, string]].concat(deviceFacts(app));
  return (
    <div class="diagnostics-page">
      <div ref={column} class="diagnostics-column" style={`transform:translateY(${-shift}px)`}>
        <div class="t-page c-fg">Diagnostics</div>
        <G kind="col" class="diagnostics-lines">
          <div class="t-section c-fg diagnostics-heading">This TV</div>
          {facts.map(([label, value], i) => (
            <F key={label} class="diagnostics-line" fid={`fact/${i}`} initial={i === 0} label={`${label}: ${value}`}>
              <span class="t-body diagnostics-label">{label}</span>
              <span class="t-body diagnostics-value">{value}</span>
            </F>
          ))}
          <div class="t-section c-fg diagnostics-heading">What it plays</div>
          {lines === null && <div class="t-body c-muted diagnostics-note">Checking what this TV plays…</div>}
          {lines === "failed" && (
            <>
              {checking ? (
                <div class="t-body c-muted diagnostics-note">Checking what this TV plays…</div>
              ) : (
                <div class="t-body c-danger diagnostics-note">The check of this TV failed.</div>
              )}
              <G kind="row" class="diagnostics-again">
                <Pill label="Check again" icon="refresh-cw" fid="again" onSelect={() => !checking && check(true)} />
              </G>
            </>
          )}
          {Array.isArray(lines) &&
            lines.map((line, i) => (
              <F key={i} class="diagnostics-line" fid={`plays/${i}`} label={line}>
                <span class="t-body diagnostics-value">{line}</span>
              </F>
            ))}
        </G>
      </div>
      <div class="rail-fade" />
    </div>
  );
}

/** "0.2.0 (1a2b3c4d5)": the app's version and the commit it was built from, when known. */
export function buildLabel(version: string): string {
  const commit = typeof __PLEY_COMMIT__ === "string" ? __PLEY_COMMIT__ : "";
  return commit && commit !== "unknown" ? `${version} (${commit})` : version;
}

/** Asks before signing out, plainly, naming the server (as on Roku): Cancel comes first, so OK pressed twice doesn't sign out. */
export function SignOutDialog(p: { server: string | null; onCancel: () => void; onSignOut: () => void }) {
  return (
    <Dialog
      title="Sign out?"
      body={p.server ? `This TV signs out of ${p.server}.` : undefined}
      onDismiss={p.onCancel}
      buttons={[
        { label: "Cancel", initial: true, onSelect: p.onCancel },
        { label: "Sign out", color: "var(--danger-text)", onSelect: p.onSignOut },
      ]}
    />
  );
}
