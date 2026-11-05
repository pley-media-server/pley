/**
 * The languages the web app is in, with i18next.
 *
 * Each language's words are in `locales/<language>/<namespace>.json`.
 * English is built into the app; the others load on demand, all of a
 * language's namespaces at once. English is also the fallback for a word
 * missing elsewhere.
 *
 * The language shown is the account's own choice, else the server's
 * (Settings → General), else the first of the browser's languages that Pley
 * has, else English ({@link pickLanguage}). The last one shown is
 * remembered on the device, so the next visit starts in it before the
 * server has answered.
 */
import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import { en } from "./resources";

export const LANGUAGES = ["en", "fr", "es"] as const;
export type Language = (typeof LANGUAGES)[number];

const STORAGE_KEY = "pley.language";

const others = import.meta.glob<{ default: Record<string, unknown> }>(["./locales/*/*.json", "!./locales/en/*.json"]);

/** The language Pley has for a tag ("fr-CA" → "fr"), if any. */
export function supportedLanguage(tag: string | null | undefined): Language | undefined {
  const base = tag?.trim().split(/[-_]/)[0]?.toLowerCase();
  return LANGUAGES.find((language) => language === base);
}

/** The first of the browser's (or the desktop's) languages that Pley has. */
export function deviceLanguage(): Language | undefined {
  const tags = typeof navigator === "undefined" ? [] : navigator.languages?.length ? navigator.languages : [navigator.language];
  for (const tag of tags) {
    const language = supportedLanguage(tag);
    if (language) return language;
  }
  return undefined;
}

/** The language to show: the account's, else the server's, else the device's, else English. */
export function pickLanguage(account: string | null | undefined, server: string | null | undefined): Language {
  return supportedLanguage(account) ?? supportedLanguage(server) ?? deviceLanguage() ?? "en";
}

function remembered(): Language | undefined {
  try {
    return supportedLanguage(localStorage.getItem(STORAGE_KEY));
  } catch {
    return undefined;
  }
}

const loaded = new Set<string>(["en"]);

/** Loads a language's words, once. */
async function load(language: Language): Promise<void> {
  if (loaded.has(language)) return;
  const prefix = `./locales/${language}/`;
  await Promise.all(
    Object.entries(others)
      .filter(([path]) => path.startsWith(prefix))
      .map(async ([path, importer]) => {
        const namespace = path.slice(prefix.length, -".json".length);
        i18next.addResourceBundle(language, namespace, (await importer()).default, true, true);
      }),
  );
  loaded.add(language);
}

void i18next.use(initReactI18next).init({
  resources: { en },
  lng: "en",
  fallbackLng: "en",
  supportedLngs: [...LANGUAGES],
  ns: Object.keys(en),
  defaultNS: "common",
  // React escapes what it renders.
  interpolation: { escapeValue: false },
  returnNull: false,
  initAsync: false,
});

/**
 * Shows the app in `language`: its words loaded first, so nothing shows in
 * English on the way. Remembered for the next visit.
 */
export async function setLanguage(language: Language): Promise<void> {
  await load(language);
  if (i18next.language !== language) await i18next.changeLanguage(language);
  document.documentElement.lang = language;
  try {
    localStorage.setItem(STORAGE_KEY, language);
  } catch {
    // Private windows may refuse: the next visit starts from the browser's language.
  }
}

/** Before the first page: the language of the last visit, else the browser's. */
export function startLanguage(): Promise<void> {
  return setLanguage(remembered() ?? deviceLanguage() ?? "en").catch(() => setLanguage("en"));
}

/** The language shown, for `Intl` (dates, numbers, language names). */
export function currentLanguage(): Language {
  return supportedLanguage(i18next.language) ?? "en";
}

export const i18n = i18next;
export const t = i18next.t.bind(i18next);
