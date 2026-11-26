import { Check, Palette } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Card, CardTitle } from "@/components/ui";
import { cn } from "@/lib/cn";
import { ACCENTLESS, ACCENTS, showAccent, showTheme, THEMES, useAccent, useTheme, type Accent, type Theme } from "@/theme";

/**
 * This device's theme, each one shown as a small Pley in its colors, and an
 * accent over it. A pick shows at once and stays on this device (src/theme);
 * another browser, or the desktop app, keeps its own.
 */
export function ThemeCard() {
  const { t } = useTranslation("pages");
  const shown = useTheme();
  const accent = useAccent();
  const accentless = shown === ACCENTLESS;

  return (
    <Card>
      <CardTitle icon={<Palette />}>{t("account.theme.title")}</CardTitle>
      <p className="mb-4 text-sm text-muted">{t("account.theme.hint")}</p>
      <div role="radiogroup" aria-label={t("account.theme.title")} className="grid gap-2 sm:grid-cols-3 sm:gap-3">
        {THEMES.map((theme) => {
          const checked = theme === shown;
          return (
            <label
              key={theme}
              className={cn(
                "group relative flex cursor-pointer items-center gap-3 rounded-lg border p-2 transition-colors has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent sm:flex-col sm:items-stretch sm:gap-2.5",
                checked ? "border-accent bg-accent/[0.06]" : "border-border hover:border-border-strong hover:bg-fg/[0.03]",
              )}
            >
              <input
                type="radio"
                name="theme"
                value={theme}
                checked={checked}
                onChange={() => showTheme(theme)}
                className="sr-only"
              />
              <ThemePreview theme={theme} />
              {checked ? (
                <span className="absolute -right-1.5 -top-1.5 flex size-5 items-center justify-center rounded-full bg-accent text-accent-fg shadow ring-2 ring-surface">
                  <Check className="size-3" strokeWidth={3} />
                </span>
              ) : null}
              <span className="min-w-0 px-1 sm:pb-0.5">
                <span className="block text-sm font-medium text-fg">{t(`account.theme.names.${theme}`)}</span>
                <span className="block text-xs leading-snug text-muted">{t(`account.theme.about.${theme}`)}</span>
              </span>
            </label>
          );
        })}
      </div>
      <h3 id="accent-title" className="mt-6 text-sm font-medium text-fg">
        {t("account.theme.accent.title")}
      </h3>
      <p className="mb-3 text-sm text-muted">
        {accentless ? t("account.theme.accent.accentless") : t("account.theme.accent.hint")}
      </p>
      <div role="radiogroup" aria-labelledby="accent-title" className="flex flex-wrap gap-2">
        {[null, ...ACCENTS].map((choice) => (
          <AccentChoice key={choice ?? "own"} accent={choice} theme={shown} checked={choice === accent} disabled={accentless} />
        ))}
      </div>
    </Card>
  );
}

/**
 * An accent to pick (null for the theme's own), its swatch drawn by the CSS
 * itself in the theme shown, so it is the color the page will have.
 */
function AccentChoice({ accent, theme, checked, disabled }: { accent: Accent | null; theme: Theme; checked: boolean; disabled: boolean }) {
  const { t } = useTranslation("pages");
  return (
    <label
      className={cn(
        "flex items-center gap-2 rounded-md border px-3 py-2 text-sm transition-colors has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent",
        disabled ? "cursor-not-allowed opacity-50" : "cursor-pointer",
        checked ? "border-accent bg-accent/[0.06] text-fg" : "border-border text-muted",
        !checked && !disabled && "hover:border-border-strong hover:text-fg",
      )}
    >
      <input
        type="radio"
        name="accent"
        value={accent ?? ""}
        checked={checked}
        disabled={disabled}
        onChange={() => showAccent(accent)}
        className="sr-only"
      />
      <span
        data-theme={theme}
        data-accent={accent ?? "own"}
        aria-hidden
        className="flex size-5 shrink-0 items-center justify-center rounded-full bg-accent text-accent-fg ring-1 ring-fg/20"
      >
        {checked ? <Check className="size-3" strokeWidth={3} /> : null}
      </span>
      {accent ? t(`account.theme.accent.names.${accent}`) : t("account.theme.accent.own")}
    </label>
  );
}

/** A small Pley in a theme's colors: its sidebar, a heading with a button, and a row of posters. */
function ThemePreview({ theme }: { theme: Theme }) {
  return (
    <div
      data-theme={theme}
      aria-hidden
      className="flex aspect-[16/9] w-28 shrink-0 overflow-hidden rounded-md bg-bg bg-(image:--glow) ring-1 ring-fg/10 sm:w-auto"
    >
      <div className="flex w-1/4 flex-col gap-1.5 border-r border-border bg-surface bg-(image:--sidebar) p-2">
        <span className="h-1.5 w-3/4 rounded-full bg-accent" />
        <span className="h-1.5 w-full rounded-full bg-fg/15" />
        <span className="h-1.5 w-2/3 rounded-full bg-fg/15" />
        <span className="h-1.5 w-5/6 rounded-full bg-fg/15" />
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-1.5 p-2">
        <span className="flex items-center gap-1">
          <span className="h-2 w-1/2 rounded-full bg-fg/80" />
          <span className="ml-auto h-2.5 w-1/4 rounded-full bg-accent" />
        </span>
        <span className="h-1.5 w-1/3 rounded-full bg-muted/50" />
        <span className="mt-auto grid grid-cols-4 gap-1">
          <span className="aspect-[2/3] rounded-sm bg-surface-3 ring-1 ring-accent" />
          <span className="aspect-[2/3] rounded-sm bg-surface-3" />
          <span className="aspect-[2/3] rounded-sm bg-surface-3" />
          <span className="aspect-[2/3] rounded-sm bg-surface-3" />
        </span>
      </div>
    </div>
  );
}
