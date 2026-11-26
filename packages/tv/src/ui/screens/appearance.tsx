import { t } from "../../i18n";
import { F, G } from "../kit/focusable";
import { Icon } from "../kit/icons";
import { ACCENTS, THEMES, themeStyle, tokenColor, type AccentId, type ThemeId } from "../themes";
import { useApp } from "./common";

/**
 * Settings → Appearance (the web's Account → Appearance): this TV's theme,
 * each a small Pley in its colors with its name and caption, then the
 * accent over it, the theme's own first. A pick shows at once and stays on
 * the TV; `accent` opens on the accents.
 */
export function AppearanceScreen(p: { accent?: boolean }) {
  const app = useApp();
  const shown = app.theme.value;
  const accent = app.accent.value;
  return (
    <div class="appearance-page">
      <div class="t-page c-fg appearance-title">{t("settings.categories.appearance")}</div>
      <div class="appearance-section" style="top:81px">
        <div class="t-section c-fg">{t("settings.theme.title")}</div>
        <div class="t-caption c-muted appearance-hint">{t("settings.theme.hint")}</div>
      </div>
      <G kind="grid" columns={3} restore class="theme-grid">
        {THEMES.map((theme) => (
          <ThemeCard key={theme} theme={theme} accent={accent} current={theme === shown} initial={!p.accent && theme === shown} onPick={() => app.chooseTheme(theme)} />
        ))}
      </G>
      <div class="appearance-section" style="top:386px">
        <div class="t-section c-fg">{t("settings.theme.accent.title")}</div>
        <div class="t-caption c-muted appearance-hint">{t("settings.theme.accent.hint")}</div>
      </div>
      <G kind="row" restore class="accent-row">
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

/** A theme to pick: its preview, its name and its caption; the one shown is ringed and ticked. */
function ThemeCard(p: { theme: ThemeId; accent: AccentId | null; current: boolean; initial: boolean; onPick: () => void }) {
  const name = t(`settings.theme.names.${p.theme}`);
  const about = t(`settings.theme.about.${p.theme}`);
  return (
    <F class={`theme-card${p.current ? " current" : ""}`} fid={`theme-${p.theme}`} initial={p.initial} label={`${name}, ${about}`} onSelect={p.onPick}>
      <div class="theme-preview-box">
        <ThemePreview theme={p.theme} accent={p.accent} />
        {p.current && (
          <span class="theme-tick">
            <Icon name="check" size={12} />
          </span>
        )}
      </div>
      <div class="theme-card-text">
        <div class="t-label one-line theme-name">{name}</div>
        <div class="t-caption lines-2 theme-about">{about}</div>
      </div>
    </F>
  );
}

/**
 * A small Pley in a theme's colors, under the accent picked: the rail (with
 * Sunshine's rainbow), a title with its button, and a row of posters, the
 * first one focused. Its colors are written on it, so it draws in its own
 * theme inside the page's.
 */
function ThemePreview(p: { theme: ThemeId; accent: AccentId | null }) {
  return (
    <div class="theme-preview" style={themeStyle(p.theme, p.accent)} aria-hidden="true">
      <div class="tp-rail">
        <span class="tp-dot on" />
        <span class="tp-dot" />
        <span class="tp-dot" />
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

/** An accent to pick (null: the theme's own), its swatch in the color it gives the theme shown. */
function AccentChip(p: { theme: ThemeId; accent: AccentId | null; current: boolean; initial: boolean; onPick: () => void }) {
  const name = p.accent ? t(`settings.theme.accent.names.${p.accent}`) : t("settings.theme.accent.own");
  return (
    <F class={`chip accent-chip${p.current ? " current" : ""}`} fid={`accent-${p.accent ?? "own"}`} initial={p.initial} label={name} onSelect={p.onPick}>
      <span class="accent-swatch" style={`background:${tokenColor(p.theme, p.accent, "accent")};color:${tokenColor(p.theme, p.accent, "accent-fg")}`}>
        {p.current && <Icon name="check" size={12} />}
      </span>
      <span class="t-label">{name}</span>
    </F>
  );
}
