import { t } from "../../i18n";
import { F, G } from "../kit/focusable";
import { Icon } from "../kit/icons";
import { ACCENTS, THEMES, themeStyle, tokenColor, type AccentId, type ThemeId } from "../themes";
import { useApp } from "./common";

/** The element of the page with this memory key, where Up or Down lands. */
const byFid = (fid: string) => document.querySelector<HTMLElement>(`[data-fid="${fid}"]`);

/**
 * Settings → Appearance (the web's Account → Appearance), as every TV app
 * lays it out: the nine themes five to a row, each a small Pley in its
 * colors over its name and caption, then the accents, the theme's own
 * first, all on one screen. A pick shows at once and stays on the TV for
 * the profile; `accent` opens on the accents. Up and Down between the two
 * land on the pick in use.
 */
export function AppearanceScreen(p: { accent?: boolean }) {
  const app = useApp();
  const shown = app.theme.value;
  const accent = app.accent.value;
  return (
    <div class="appearance-page">
      <div class="t-page c-fg appearance-title">{t("settings.categories.appearance")}</div>
      <div class="appearance-section" style="top:69px">
        <span class="t-body-large c-fg">{t("settings.theme.title")}</span>
        <span class="t-body c-muted appearance-hint">{t("settings.theme.hint")}</span>
      </div>
      <G kind="grid" columns={5} restore class="theme-grid" onEnter={(dir) => (dir === "up" ? byFid(`theme-${shown}`) : null)}>
        {THEMES.map((theme) => (
          <ThemeCard key={theme} theme={theme} accent={accent} current={theme === shown} initial={!p.accent && theme === shown} onPick={() => app.chooseTheme(theme)} />
        ))}
      </G>
      <div class="appearance-section" style="top:434px">
        <span class="t-body-large c-fg">{t("settings.theme.accent.title")}</span>
        <span class="t-body c-muted appearance-hint">{t("settings.theme.accent.hint")}</span>
      </div>
      <G kind="row" restore class="accent-row" onEnter={(dir) => (dir === "down" ? byFid(`accent-${accent ?? "own"}`) : null)}>
        {[null, ...ACCENTS].map((choice) => (
          <AccentChip
            key={choice ?? "own"}
            theme={shown}
            accent={choice}
            current={choice === accent}
            initial={!!p.accent && choice === accent}
            onPick={() => app.chooseAccent(choice)}
          />
        ))}
      </G>
    </div>
  );
}

/** A theme to pick: its preview over its name and caption; the one shown has the accent's border and a tick at its top right. */
function ThemeCard(p: { theme: ThemeId; accent: AccentId | null; current: boolean; initial: boolean; onPick: () => void }) {
  const name = t(`settings.theme.names.${p.theme}`);
  const about = t(`settings.theme.about.${p.theme}`);
  return (
    <F class={`theme-card${p.current ? " current" : ""}`} fid={`theme-${p.theme}`} initial={p.initial} label={`${name}, ${about}`} onSelect={p.onPick}>
      <ThemePreview theme={p.theme} accent={p.accent} />
      <div class="t-body one-line theme-name">{name}</div>
      <div class="t-caption lines-2 theme-about">{about}</div>
      {p.current && (
        <span class="theme-tick">
          <Icon name="check" size={14} />
        </span>
      )}
      <div class="theme-ring" />
    </F>
  );
}

/**
 * A small Pley in a theme's colors, under the accent picked: the rail (with
 * Sunshine's rainbow), a heading with its button, and a row of posters, the
 * first one focused. Its colors are written on it, so it draws in its own
 * theme inside the page's.
 */
function ThemePreview(p: { theme: ThemeId; accent: AccentId | null }) {
  return (
    <div class="theme-preview" style={themeStyle(p.theme, p.accent)} aria-hidden="true">
      <div class="tp-rail">
        <span class="on" />
        <span />
        <span />
        <span />
      </div>
      <div class="tp-page">
        <span class="tp-title" />
        <span class="tp-line" />
        <span class="tp-button" />
        <div class="tp-posters">
          <span class="focused" />
          <span />
          <span />
          <span />
        </div>
      </div>
    </div>
  );
}

/** An accent to pick (null: the theme's own), its swatch in the color it gives the theme shown; the one in use is filled with it and ticked. */
function AccentChip(p: { theme: ThemeId; accent: AccentId | null; current: boolean; initial: boolean; onPick: () => void }) {
  const name = p.accent ? t(`settings.theme.accent.names.${p.accent}`) : t("settings.theme.accent.own");
  return (
    <F class={`accent-chip${p.current ? " current" : ""}`} fid={`accent-${p.accent ?? "own"}`} initial={p.initial} label={name} onSelect={p.onPick}>
      {p.current ? (
        <span class="accent-swatch picked">
          <Icon name="check" size={12} />
        </span>
      ) : (
        <span class="accent-swatch" style={`background:${tokenColor(p.theme, p.accent, "accent")}`} />
      )}
      <span class="t-label">{name}</span>
      <div class="theme-ring" />
    </F>
  );
}
