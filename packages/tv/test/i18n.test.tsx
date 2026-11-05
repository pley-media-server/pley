import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { render } from "preact";
import { afterEach, describe, expect, it } from "vitest";
import { catalogs } from "../src/i18n/catalogs";
import {
  currentLanguage,
  currentLocale,
  formatDate,
  formatNumber,
  LANGUAGES,
  navigatorLanguages,
  pickLanguage,
  pluralForm,
  setDeviceLanguages,
  setLanguage,
  supportedLanguage,
  t,
  Trans,
  type Key,
} from "../src/i18n";
// Shared with the native Samsung app's tests, which must answer the same.
import fixture from "./fixtures/language.json";

const LOCALES = join(dirname(fileURLToPath(import.meta.url)), "../src/i18n/locales");

type Words = Record<string, string>;

/** `{"a": {"b": "x"}}` → `{"a.b": "x"}`. */
function flatten(value: unknown, prefix = "", into: Words = {}): Words {
  if (typeof value === "string") into[prefix] = value;
  else if (value && typeof value === "object") {
    for (const [key, inner] of Object.entries(value)) flatten(inner, prefix ? `${prefix}.${key}` : key, into);
  }
  return into;
}

function areas(language: string): string[] {
  return readdirSync(join(LOCALES, language))
    .filter((name) => name.endsWith(".json"))
    .map((name) => name.slice(0, -".json".length))
    .sort();
}

function file(language: string, area: string): unknown {
  return JSON.parse(readFileSync(join(LOCALES, language, `${area}.json`), "utf8"));
}

const PLURAL = /_(zero|one|two|few|many|other)$/;

/** The words' `{{values}}`, sorted. */
function values(text: string): string[] {
  return (text.match(/\{\{\s*[^,}\s]+/g) ?? []).map((m) => m.replace(/^\{\{\s*/, "")).sort();
}

/** The words' markup, `<name>` and `</name>`, sorted. */
function tags(text: string): string[] {
  return (text.match(/<\/?[a-z][a-z0-9]*>/g) ?? []).sort();
}

/**
 * What a language needs for English's words: the same keys, and for a key
 * counted with `_one`/`_other` in English every form the language counts
 * in (French and Spanish have `_many` too, for whole millions).
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

afterEach(() => {
  setLanguage("en");
  setDeviceLanguages(navigatorLanguages);
});

describe("the languages' words", () => {
  it("every language has English's areas, each one in the app", () => {
    for (const language of LANGUAGES) {
      expect(areas(language), `locales/${language}`).toEqual(areas("en"));
      expect(Object.keys(catalogs[language]).sort(), `catalogs.ts, ${language}`).toEqual(areas("en"));
      for (const area of areas(language)) {
        expect((catalogs[language] as Record<string, unknown>)[area], `catalogs.ts takes ${language}/${area}.json`).toEqual(file(language, area));
      }
    }
  });

  for (const area of areas("en")) {
    const english = flatten(file("en", area));

    it(`en/${area}.json counts with _one and _other together, and writes something for each`, () => {
      for (const [key, text] of Object.entries(english)) {
        expect(text.trim(), `en:${area}:${key} is empty`).not.toBe("");
        if (!PLURAL.test(key)) continue;
        const base = key.replace(PLURAL, "");
        expect(english[`${base}_one`], `${area}:${base}_one`).toBeDefined();
        expect(english[`${base}_other`], `${area}:${base}_other`).toBeDefined();
      }
    });

    for (const language of LANGUAGES.filter((l) => l !== "en")) {
      it(`${language}/${area}.json has every word English has, with the same values and markup`, () => {
        const words = flatten(file(language, area));
        expect(Object.keys(words).sort()).toEqual(expectedKeys(language, english));
        for (const [key, text] of Object.entries(words)) {
          expect(text.trim(), `${language}:${area}:${key} is empty`).not.toBe("");
          const source = english[key] ?? english[key.replace(PLURAL, "_other")] ?? "";
          // A form for one may leave the count out ("un film").
          const counted = (v: string) => !(v === "count" && /_one$/.test(key));
          expect(values(text).filter(counted), `${language}:${area}:${key}`).toEqual(values(source).filter(counted));
          expect(tags(text), `${language}:${area}:${key}`).toEqual(tags(source));
        }
      });
    }

    it(`fr/${area}.json writes French's apostrophes and no-break spaces`, () => {
      for (const [key, text] of Object.entries(flatten(file("fr", area)))) {
        expect(text, `fr:${area}:${key}: ’, not '`).not.toMatch(/'/);
        expect(text, `fr:${area}:${key}: a no-break space before : ; ? ! »`).not.toMatch(/ [:;?!»]/);
        expect(text, `fr:${area}:${key}: a no-break space after «`).not.toMatch(/« /);
      }
    });

    it(`es/${area}.json opens its questions and exclamations`, () => {
      for (const [key, text] of Object.entries(flatten(file("es", area)))) {
        expect((text.match(/¿/g) ?? []).length, `es:${area}:${key}`).toBe((text.match(/\?/g) ?? []).length);
        expect((text.match(/¡/g) ?? []).length, `es:${area}:${key}`).toBe((text.match(/!/g) ?? []).length);
      }
    });
  }
});

describe("t", () => {
  it("writes the language shown, English to start with", () => {
    expect(t("onboarding.servers.title")).toBe("Choose your server");
    setLanguage("fr");
    expect(currentLanguage()).toBe("fr");
    expect(document.documentElement.lang).toBe("fr");
    expect(t("onboarding.servers.title")).toBe("Choisissez votre serveur");
    setLanguage("es");
    expect(t("onboarding.servers.title")).toBe("Elige tu servidor");
  });

  it("fills in the values", () => {
    expect(t("onboarding.servers.forgetTitle", { name: "Den" })).toBe("Forget Den?");
    setLanguage("fr");
    expect(t("onboarding.servers.forgetTitle", { name: "Den" })).toBe("Oublier Den ?");
    // A value is written as it is, whatever it holds.
    expect(t("onboarding.link.signedInAs", { name: "{{name}} <b>" })).toBe("Connecté en tant que {{name}} <b>");
  });

  it("counts in each language's forms", () => {
    const digits = (n: number) => t("onboarding.profiles.digits", { count: n });
    expect([0, 1, 2].map(digits)).toEqual(["0 digits entered", "1 digit entered", "2 digits entered"]);
    setLanguage("fr");
    expect([0, 1, 2].map(digits)).toEqual(["0 chiffre saisi", "1 chiffre saisi", "2 chiffres saisis"]);
    setLanguage("es");
    expect([0, 1, 2].map(digits)).toEqual(["0 dígitos introducidos", "1 dígito introducido", "2 dígitos introducidos"]);
  });

  it("leaves the markup out of plain words", () => {
    expect(t("onboarding.link.goTo", { address: "pley.local/link" })).toBe("or go to pley.local/link");
  });

  it("answers the key itself for a key it doesn't have", () => {
    expect(t("onboarding.servers.nothing" as Key)).toBe("onboarding.servers.nothing");
  });

  it("writes English where a language lacks a word", () => {
    const actions = catalogs.es.common as { actions: Record<string, string> };
    const kept = actions.actions.retry;
    delete actions.actions.retry;
    try {
      setLanguage("es");
      expect(t("common.actions.retry")).toBe("Retry");
      expect(t("common.actions.cancel")).toBe("Cancelar");
    } finally {
      actions.actions.retry = kept!;
    }
  });
});

describe("Trans", () => {
  it("draws the markup in the words, the values as text", () => {
    const root = document.createElement("div");
    const draw = (address: string) =>
      render(<Trans k="onboarding.link.goTo" values={{ address }} tags={{ address: (text) => <span class="link-address">{text}</span> }} />, root);
    draw("pley.local/link");
    expect(root.innerHTML).toBe('or go to <span class="link-address">pley.local/link</span>');
    draw("<b>x</b>");
    expect(root.querySelector("b")).toBeNull();
    expect(root.textContent).toBe("or go to <b>x</b>");
    setLanguage("fr");
    draw("pley.local/link");
    expect(root.innerHTML).toBe('ou allez sur <span class="link-address">pley.local/link</span>');
    render(null, root);
  });
});

describe("plural forms", () => {
  it("are CLDR's, as the engines that have Intl.PluralRules say", () => {
    const numbers = [0, 0.5, 1, 1.5, 2, 3, 11, 21, 100, 999_999, 1_000_000, 1_000_001, 1_500_000, 2_000_000, 1e9, -1, -2];
    for (const language of LANGUAGES) {
      const rules = new Intl.PluralRules(language);
      for (const n of numbers) expect([language, n, pluralForm(language, n)]).toEqual([language, n, rules.select(n)]);
    }
  });
});

describe("the language shown", () => {
  it("is the account's, else the server's, else the TV's, else English, as the shared fixture has it", () => {
    for (const c of fixture.pickLanguage) {
      expect([c, pickLanguage(c.account, c.server, c.device)]).toEqual([c, c.out]);
    }
  });

  it("matches tags by their language", () => {
    expect(supportedLanguage("fr-CA")).toBe("fr");
    expect(supportedLanguage("ES_es")).toBe("es");
    expect(supportedLanguage("de")).toBeUndefined();
    expect(supportedLanguage(null)).toBeUndefined();
    expect(supportedLanguage("")).toBeUndefined();
  });

  it("writes dates and numbers the TV's way when it is in the language shown", () => {
    setDeviceLanguages(() => ["en-GB", "fr_CA"]);
    expect(currentLocale()).toBe("en-GB");
    expect(formatDate(new Date(2023, 3, 1), { day: "numeric", month: "short", year: "numeric" })).toBe("1 Apr 2023");
    setLanguage("fr");
    expect(currentLocale()).toBe("fr-CA");
    expect(formatNumber(1234567)).toBe(new Intl.NumberFormat("fr-CA").format(1234567));
    setLanguage("es");
    expect(currentLocale()).toBe("es");
    expect(formatNumber(1234.5)).toBe(new Intl.NumberFormat("es").format(1234.5));
  });
});
