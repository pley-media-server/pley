/**
 * The languages of the app's own pages (choosing a server, the downloads
 * it keeps), with i18next. Each language's words are in
 * `locales/<language>.json`, all built in; English is also the fallback
 * for a word missing elsewhere.
 *
 * These pages show before any server is chosen, so they are in the
 * desktop's language: the first of its languages that Pley has, else
 * English. A server's web app, once open, picks its own. The `notices` are
 * the system's notifications, which the Rust side says in the language the
 * server's pages are in (src-tauri/src/words.rs).
 */
import i18next from "i18next";
import en from "./locales/en.json";
import es from "./locales/es.json";
import fr from "./locales/fr.json";

export const LANGUAGES = ["en", "fr", "es"] as const;
export type Language = (typeof LANGUAGES)[number];

/** The words of each language, English first: what the others are checked against (`i18n.test.ts`). */
export const CATALOGS: Record<Language, typeof en> = { en, fr, es };

/** The language Pley has for a tag ("fr-CA" → "fr"), if any. */
export function supportedLanguage(tag: string | null | undefined): Language | undefined {
  const base = tag?.trim().split(/[-_]/)[0]?.toLowerCase();
  return LANGUAGES.find((language) => language === base);
}

/** The first of the desktop's languages that Pley has. */
export function deviceLanguage(): Language | undefined {
  const tags = typeof navigator === "undefined" ? [] : navigator.languages?.length ? navigator.languages : [navigator.language];
  for (const tag of tags) {
    const language = supportedLanguage(tag);
    if (language) return language;
  }
  return undefined;
}

void i18next.init({
  resources: Object.fromEntries(LANGUAGES.map((language) => [language, { translation: CATALOGS[language] }])),
  lng: "en",
  fallbackLng: "en",
  supportedLngs: [...LANGUAGES],
  // The pages set text, never HTML.
  interpolation: { escapeValue: false },
  returnNull: false,
  initAsync: false,
});

/** Before the first words: the pages in the desktop's language. */
export async function startLanguage(): Promise<void> {
  const language = deviceLanguage() ?? "en";
  await i18next.changeLanguage(language);
  document.documentElement.lang = language;
}

/** The language shown, for `Intl` (numbers). */
export function currentLanguage(): Language {
  return supportedLanguage(i18next.language) ?? "en";
}

export const i18n = i18next;
export const t = i18next.t.bind(i18next);

/**
 * Words the app's Rust side writes for people (src-tauri/src/said.rs): a
 * key of `said`, with what goes in its values.
 */
export type Said = { key: keyof typeof en.said } & Record<string, string | number>;

/** Whether `value` is words of the Rust side's, rather than an error's text. */
export function isSaid(value: unknown): value is Said {
  const key = (value as { key?: unknown } | null)?.key;
  return typeof key === "string" && Object.hasOwn(en.said, key);
}

/** `said` in the language shown. */
export function say({ key, ...values }: Said): string {
  return t(`said.${key}`, values);
}
