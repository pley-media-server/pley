/**
 * The languages the web app is in, with i18next.
 *
 * Each language's words are in `locales/<language>/<namespace>.json`.
 * English is in the main bundle, but for the namespaces of pages that load
 * on demand (`LazyResources`), which come with those pages
 * ({@link loadNamespaces}). The other languages load when first shown, all
 * the namespaces wanted so far at once. English is also the fallback for a
 * word missing elsewhere.
 *
 * The language shown is the account's own choice, else the server's
 * (Settings → General), else the first of the browser's languages that Pley
 * has, else English ({@link pickLanguage}). The last one shown is
 * remembered on the device, so the next visit starts in it before the
 * server has answered.
 */
import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import { en, type LazyNamespace } from "./resources";

export const LANGUAGES = ["en", "fr", "es"] as const;
export type Language = (typeof LANGUAGES)[number];

const STORAGE_KEY = "pley.language";

/** Every catalog the main bundle doesn't carry, to load when wanted. */
const catalogs = import.meta.glob<{ default: Record<string, unknown> }>([
  "./locales/*/*.json",
  "!./locales/en/common.json",
  "!./locales/en/components.json",
  "!./locales/en/item.json",
  "!./locales/en/pages.json",
  "!./locales/en/player.json",
  "!./locales/en/requests.json",
]);

/** The language Pley has for a tag ("fr-CA" → "fr"), if any. */
export function supportedLanguage(tag: string | null | undefined): Language | undefined {
  const base = tag?.trim().split(/[-_]/)[0]?.toLowerCase();
  return LANGUAGES.find((language) => language === base);
}

/** The browser's (or the desktop's) languages, the one it prefers first. */
function deviceTags(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  return navigator.languages?.length ? navigator.languages : navigator.language ? [navigator.language] : [];
}

/** The first of the browser's (or the desktop's) languages that Pley has. */
export function deviceLanguage(): Language | undefined {
  for (const tag of deviceTags()) {
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

/** The namespaces every language shown needs: the main bundle's, and the lazy ones asked for so far. */
const wanted = new Set<string>(Object.keys(en));

/** Loads `language`'s words of `namespaces` it doesn't have yet. */
async function load(language: Language, namespaces: Iterable<string> = wanted): Promise<void> {
  await Promise.all(
    [...namespaces]
      .filter((namespace) => !i18next.hasResourceBundle(language, namespace))
      .map(async (namespace) => {
        const importer = catalogs[`./locales/${language}/${namespace}.json`];
        if (importer) i18next.addResourceBundle(language, namespace, (await importer()).default, true, true);
      }),
  );
}

/**
 * The words of pages whose code loads on demand, in English and the
 * language shown: their route waits for them with the code, so they show
 * complete. A language shown later loads them too.
 */
export async function loadNamespaces(namespaces: readonly LazyNamespace[]): Promise<void> {
  namespaces.forEach((namespace) => wanted.add(namespace));
  await Promise.all([...new Set<Language>(["en", currentLanguage()])].map((language) => load(language, namespaces)));
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

/** The language shown. */
export function currentLanguage(): Language {
  return supportedLanguage(i18next.language) ?? "en";
}

/**
 * The locale dates and numbers are written in: the browser's own when it is
 * in the language shown, so "en-GB" keeps its day before the month and its
 * 24-hour clock, "fr-CA" its own ways; else the language.
 */
export function currentLocale(): string {
  const language = currentLanguage();
  for (const tag of deviceTags()) {
    if (supportedLanguage(tag) !== language) continue;
    try {
      return Intl.getCanonicalLocales(tag)[0] ?? language;
    } catch {
      return language;
    }
  }
  return language;
}

export const i18n = i18next;
export const t = i18next.t.bind(i18next);
