import { afterEach, describe, expect, it, vi } from "vitest";
import { CATALOGS, deviceLanguage, isSaid, LANGUAGES, say, supportedLanguage } from "./i18n";

type Words = Record<string, string>;

/** `{"a": {"b": "x"}}` → `{"a.b": "x"}`. */
function flatten(value: unknown, prefix = "", into: Words = {}): Words {
  if (typeof value === "string") into[prefix] = value;
  else if (value && typeof value === "object") {
    for (const [key, inner] of Object.entries(value)) flatten(inner, prefix ? `${prefix}.${key}` : key, into);
  }
  return into;
}

const PLURAL = /_(zero|one|two|few|many|other)$/;

/** The words' `{{values}}`, sorted. */
function values(text: string): string[] {
  return [...text.matchAll(/\{\{\s*([^,}\s]+)[^}]*\}\}/g)].map((m) => m[1] ?? "").sort();
}

/**
 * What each language needs for English's words: the same keys, and for a
 * key counted with `_one`/`_other` in English, every form the language
 * counts in (French and Spanish have `_many` too, for a million and up).
 */
function expectedKeys(language: string, english: Words): string[] {
  const forms = new Intl.PluralRules(language).resolvedOptions().pluralCategories;
  const keys = new Set<string>();
  for (const key of Object.keys(english)) {
    if (PLURAL.test(key)) for (const form of forms) keys.add(key.replace(PLURAL, `_${form}`));
    else keys.add(key);
  }
  return [...keys].sort();
}

describe("the languages' words", () => {
  const english = flatten(CATALOGS.en);

  for (const language of LANGUAGES.filter((l) => l !== "en")) {
    it(`locales/${language}.json has every word English has, with the same values`, () => {
      const words = flatten(CATALOGS[language]);
      expect(Object.keys(words).sort()).toEqual(expectedKeys(language, english));
      for (const [key, text] of Object.entries(words)) {
        expect(text.trim(), `${language}:${key} is empty`).not.toBe("");
        const source = english[key] ?? english[key.replace(PLURAL, "_other")] ?? "";
        // A form for one may leave the count out ("un film").
        const expected = values(source).filter((v) => !(v === "count" && /_one$/.test(key)));
        expect(values(text).filter((v) => !(v === "count" && /_one$/.test(key))), `${language}:${key}`).toEqual(expected);
      }
    });
  }

  it("locales/en.json counts with _one and _other together", () => {
    for (const key of Object.keys(english).filter((k) => PLURAL.test(k))) {
      const base = key.replace(PLURAL, "");
      expect(english[`${base}_one`], `${base}_one`).toBeDefined();
      expect(english[`${base}_other`], `${base}_other`).toBeDefined();
    }
  });
});

describe("the desktop's language", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("is the first of its languages that Pley has, else none", () => {
    vi.stubGlobal("navigator", { language: "de-DE", languages: ["de-DE", "es-MX", "fr-FR"] });
    expect(deviceLanguage()).toBe("es");
    vi.stubGlobal("navigator", { language: "fr-CA", languages: [] });
    expect(deviceLanguage()).toBe("fr");
    vi.stubGlobal("navigator", { language: "de-DE", languages: ["de-DE"] });
    expect(deviceLanguage()).toBeUndefined();
  });

  it("matches tags by their language", () => {
    expect(supportedLanguage("fr-CA")).toBe("fr");
    expect(supportedLanguage("ES_es")).toBe("es");
    expect(supportedLanguage("de")).toBeUndefined();
    expect(supportedLanguage(null)).toBeUndefined();
  });
});

describe("what the Rust side says", () => {
  it("is told by its key, with its values", () => {
    const said: unknown = { key: "answeredHttp", status: 502 };
    expect(isSaid(said) && say(said)).toBe("it answered HTTP 502");
    expect(isSaid({ key: "toString" })).toBe(false);
    expect(isSaid("nothing answers there")).toBe(false);
    expect(isSaid(null)).toBe(false);
  });
});
