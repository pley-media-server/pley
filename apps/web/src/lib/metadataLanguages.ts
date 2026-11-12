import { currentLocale } from "@/i18n";

/** The languages titles and summaries can come in, as TMDB and TheTVDB take them. */
export const METADATA_LANGUAGES = [
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

/**
 * A metadata language's name in the language shown ("British English",
 * "Français (France)"), with a capital as a name on its own has, else its
 * code.
 */
export function metadataLanguageName(tag: string): string {
  const locale = currentLocale();
  try {
    const name = new Intl.DisplayNames([locale], { type: "language" }).of(tag) ?? tag;
    return name.charAt(0).toLocaleUpperCase(locale) + name.slice(1);
  } catch {
    return tag;
  }
}

/** The metadata languages, by name in the language shown, with `current` among them. */
export function metadataLanguageChoices(current: string): { value: string; label: string }[] {
  const tags = METADATA_LANGUAGES.includes(current) ? METADATA_LANGUAGES : [current, ...METADATA_LANGUAGES];
  const collator = new Intl.Collator(currentLocale());
  return tags.map((value) => ({ value, label: metadataLanguageName(value) })).sort((a, b) => collator.compare(a.label, b.label));
}

/**
 * The metadata language a new server starts from: the first of the
 * browser's languages Pley offers, as it is ("pt-BR"), else in `country`
 * ("fr" in France), else its first ("de" is "de-DE"); else US English.
 */
export function browserMetadataLanguage(tags: readonly string[], country: string): string {
  const offered = (tag: string) => METADATA_LANGUAGES.find((m) => m.toLowerCase() === tag.toLowerCase());
  for (const tag of tags) {
    const [language, region] = tag.replace("_", "-").split("-");
    if (!language) continue;
    const found =
      (region ? offered(`${language}-${region}`) : undefined) ??
      offered(`${language}-${country}`) ??
      METADATA_LANGUAGES.find((m) => m.split("-")[0] === language.toLowerCase());
    if (found) return found;
  }
  return "en-US";
}
