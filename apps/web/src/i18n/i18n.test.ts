import { describe, expect, it } from "vitest";
import { LANGUAGES, pickLanguage, supportedLanguage } from ".";
import { en } from "./resources";

const files = import.meta.glob<{ default: unknown }>("./locales/*/*.json", { eager: true });

type Words = Record<string, string>;

/** `{"a": {"b": "x"}}` → `{"a.b": "x"}`. */
function flatten(value: unknown, prefix = "", into: Words = {}): Words {
  if (typeof value === "string") into[prefix] = value;
  else if (value && typeof value === "object") {
    for (const [key, inner] of Object.entries(value)) flatten(inner, prefix ? `${prefix}.${key}` : key, into);
  }
  return into;
}

function catalog(language: string, namespace: string): Words | undefined {
  const file = files[`./locales/${language}/${namespace}.json`];
  return file ? flatten(file.default) : undefined;
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
  for (const namespace of Object.keys(en)) {
    const english = flatten(en[namespace as keyof typeof en]);

    for (const language of LANGUAGES.filter((l) => l !== "en")) {
      it(`${language}/${namespace}.json has every word English has, with the same values`, () => {
        const words = catalog(language, namespace);
        expect(words, `locales/${language}/${namespace}.json is missing`).toBeDefined();
        expect(Object.keys(words ?? {}).sort()).toEqual(expectedKeys(language, english));
        for (const [key, text] of Object.entries(words ?? {})) {
          expect(text.trim(), `${language}:${namespace}:${key} is empty`).not.toBe("");
          const source = english[key] ?? english[key.replace(PLURAL, "_other")] ?? "";
          // A form for one may leave the count out ("un film").
          const expected = values(source).filter((v) => !(v === "count" && /_one$/.test(key)));
          expect(values(text).filter((v) => !(v === "count" && /_one$/.test(key))), `${language}:${namespace}:${key}`).toEqual(expected);
        }
      });
    }

    it(`en/${namespace}.json counts with _one and _other together`, () => {
      for (const key of Object.keys(english).filter((k) => PLURAL.test(k))) {
        const base = key.replace(PLURAL, "");
        expect(english[`${base}_one`], `${namespace}:${base}_one`).toBeDefined();
        expect(english[`${base}_other`], `${namespace}:${base}_other`).toBeDefined();
      }
    });
  }

  it("every language has the same namespaces", () => {
    const namespaces = (language: string) =>
      Object.keys(files)
        .filter((path) => path.startsWith(`./locales/${language}/`))
        .map((path) => path.split("/").pop())
        .sort();
    for (const language of LANGUAGES) expect(namespaces(language)).toEqual(namespaces("en"));
  });
});

describe("pickLanguage", () => {
  it("takes the account's, then the server's, then the device's, then English", () => {
    const languages = Object.getOwnPropertyDescriptor(Navigator.prototype, "languages");
    Object.defineProperty(navigator, "languages", { value: ["de-DE", "es-MX"], configurable: true });
    try {
      expect(pickLanguage("fr", "es")).toBe("fr");
      expect(pickLanguage(null, "fr")).toBe("fr");
      expect(pickLanguage(null, null)).toBe("es");
      Object.defineProperty(navigator, "languages", { value: ["de-DE"], configurable: true });
      expect(pickLanguage(undefined, "")).toBe("en");
    } finally {
      delete (navigator as { languages?: unknown }).languages;
      if (languages) Object.defineProperty(Navigator.prototype, "languages", languages);
    }
  });

  it("matches tags by their language", () => {
    expect(supportedLanguage("fr-CA")).toBe("fr");
    expect(supportedLanguage("ES_es")).toBe("es");
    expect(supportedLanguage("de")).toBeUndefined();
    expect(supportedLanguage(null)).toBeUndefined();
  });
});
