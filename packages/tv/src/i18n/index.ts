/**
 * The languages the Samsung app is in: English, French and Spanish.
 *
 * Each language's words are in `locales/<language>/<area>.json`, in the
 * web app's format (nested keys, `{{name}}`, `_one`/`_many`/`_other`),
 * all of them in the app's script ({@link catalogs}). A key is its area
 * and its path, "onboarding.servers.title", typed from the English files,
 * so a wrong one fails the type check. English stands in for a word
 * missing elsewhere.
 *
 * The TVs' engines (Chromium 56 on Tizen 4) have no Intl.PluralRules and
 * no Intl.RelativeTimeFormat, so the plural forms are written here, as
 * CLDR has them, and i18next isn't used.
 *
 * `t()` reads the language shown from a signal: a component that calls it
 * while drawing (or calls something that does) is drawn again when the
 * language changes.
 */
import { createElement, Fragment, type ComponentChildren, type VNode } from "preact";
import { signal, type ReadonlySignal } from "@preact/signals";
import { catalogs, type Catalog } from "./catalogs";

export const LANGUAGES = ["en", "fr", "es"] as const;
export type Language = (typeof LANGUAGES)[number];

/** The language Pley has for a tag ("fr-CA", "FR_ca" → "fr"), if any. */
export function supportedLanguage(tag: string | null | undefined): Language | undefined {
  const base = (tag ?? "").trim().split(/[-_]/)[0]!.toLowerCase();
  return LANGUAGES.filter((language) => language === base)[0];
}

/** The first of the device's languages (its preferred first) that Pley has. */
export function deviceLanguage(tags: readonly string[]): Language | undefined {
  for (const tag of tags) {
    const language = supportedLanguage(tag);
    if (language) return language;
  }
  return undefined;
}

/**
 * The language to show: the account's, else the server's, else the first
 * of the device's that Pley has, else English. Anything Pley isn't in
 * counts as none (packages/tv/test/fixtures/language.json).
 */
export function pickLanguage(account: string | null | undefined, server: string | null | undefined, device: readonly string[]): Language {
  return supportedLanguage(account) ?? supportedLanguage(server) ?? deviceLanguage(device) ?? "en";
}

/** The browser engine's languages, its preferred first. */
export function navigatorLanguages(): string[] {
  if (typeof navigator === "undefined") return [];
  if (navigator.languages && navigator.languages.length) return navigator.languages.slice();
  return navigator.language ? [navigator.language] : [];
}

let deviceTags: () => readonly string[] = navigatorLanguages;

/** Where the device's languages come from (the app's platform): for the region dates and numbers are written in. */
export function setDeviceLanguages(tags: () => readonly string[]): void {
  deviceTags = tags;
}

const shown = signal<Language>("en");

/** The language shown, as a signal; {@link setLanguage} changes it. */
export const language: ReadonlySignal<Language> = shown;

/** The language shown; a component that reads it while drawing is drawn again when it changes. */
export function currentLanguage(): Language {
  return shown.value;
}

/** Shows the app in `language` from now on: every component that wrote words draws them again. */
export function setLanguage(next: Language): void {
  if (typeof document !== "undefined") document.documentElement.lang = next;
  shown.value = next;
}

/**
 * The locale dates and numbers are written in: the device's own when it is
 * in the language shown, so "en-GB" keeps its day before the month and its
 * 24-hour clock; else the language.
 */
export function currentLocale(): string {
  const lang = shown.value;
  const tags = deviceTags();
  for (const raw of tags) {
    if (supportedLanguage(raw) !== lang) continue;
    const tag = raw.trim().replace(/_/g, "-");
    try {
      return Intl.getCanonicalLocales(tag)[0] ?? lang;
    } catch {
      return lang;
    }
  }
  return lang;
}

// ---- counting

export type PluralForm = "one" | "many" | "other";

/**
 * The plural form of `n` in `language`, as CLDR has it: English "one" is
 * exactly 1; French "one" is 0 to 1.99…; Spanish "one" is 1. French and
 * Spanish say "many" for a whole number of millions (1 000 000, 2 000 000…).
 */
export function pluralForm(lang: Language, n: number): PluralForm {
  const abs = Math.abs(n);
  const whole = Math.floor(abs);
  const integer = whole === abs;
  if (lang === "en") return abs === 1 ? "one" : "other";
  if (lang === "fr" ? whole === 0 || whole === 1 : abs === 1) return "one";
  return integer && whole !== 0 && whole % 1_000_000 === 0 ? "many" : "other";
}

// ---- numbers and dates

const numberFormats: Record<string, Intl.NumberFormat> = {};
const dateFormats: Record<string, Intl.DateTimeFormat> = {};

/** `n` as the language shown writes it: "12,345", "12 345", "12.345". */
export function formatNumber(n: number, options?: Intl.NumberFormatOptions): string {
  const locale = currentLocale();
  const key = options ? `${locale} ${JSON.stringify(options)}` : locale;
  try {
    const format = (numberFormats[key] ??= new Intl.NumberFormat(locale, options));
    return format.format(n);
  } catch {
    return String(n);
  }
}

/** `date` as the language shown writes it, with Intl.DateTimeFormat's `options` ({ month: "short", day: "numeric" }). */
export function formatDate(date: Date | number, options: Intl.DateTimeFormatOptions): string {
  const locale = currentLocale();
  const key = `${locale} ${JSON.stringify(options)}`;
  const at = typeof date === "number" ? new Date(date) : date;
  try {
    const format = (dateFormats[key] ??= new Intl.DateTimeFormat(locale, options));
    return format.format(at);
  } catch {
    return at.toDateString();
  }
}

// ---- words

type Plural = "one" | "many" | "other";
/** "count_one" → "count". */
type Base<K extends string> = K extends `${infer B}_${Plural}` ? B : K;
type Leaves<T, P extends string> = {
  [K in keyof T & string]: T[K] extends string ? `${P}${Base<K>}` : Leaves<T[K], `${P}${K}.`>;
}[keyof T & string];
type Counted<T, P extends string> = {
  [K in keyof T & string]: T[K] extends string ? (K extends `${infer B}_other` ? `${P}${B}` : never) : Counted<T[K], `${P}${K}.`>;
}[keyof T & string];

/** Every key there is: "onboarding.servers.title". */
export type Key = { [A in keyof Catalog & string]: Leaves<Catalog[A], `${A}.`> }[keyof Catalog & string];
/** The keys that count (`_one`/`_other` in English), which need a `count`. */
export type CountKey = { [A in keyof Catalog & string]: Counted<Catalog[A], `${A}.`> }[keyof Catalog & string];

/** What a key's `{{names}}` stand for. */
export type Values = Record<string, string | number>;
type ValuesFor<K extends Key> = K extends CountKey ? [values: Values & { count: number }] : [values?: Values];

/** The words at `key` ("onboarding.servers.title") in `lang`'s catalogs, if there are some. */
function lookup(lang: Language, key: string): string | undefined {
  let at: unknown = catalogs[lang];
  const parts = key.split(".");
  for (let i = 0; i < parts.length; i++) {
    if (!at || typeof at !== "object") return undefined;
    at = (at as Record<string, unknown>)[parts[i]!];
  }
  return typeof at === "string" ? at : undefined;
}

/** The words of `key` before their values: in `lang`, else English; the plural form `count` takes. */
function template(lang: Language, key: string, count: number | undefined): string | undefined {
  const tryIn = (l: Language): string | undefined => {
    if (count !== undefined) {
      const counted = lookup(l, `${key}_${pluralForm(l, count)}`) ?? lookup(l, `${key}_other`);
      if (counted !== undefined) return counted;
    }
    return lookup(l, key);
  };
  return tryIn(lang) ?? (lang === "en" ? undefined : tryIn("en"));
}

const VALUE = /\{\{\s*([A-Za-z0-9_]+)\s*(?:,\s*([a-z]+)\s*)?\}\}/g;
const TAG = /<(\/?)([a-z][a-z0-9]*)>/g;

/** `text` with its `{{values}}` written in; `{{count, number}}` as the language writes numbers. */
function fill(text: string, values: Values | undefined): string {
  if (text.indexOf("{{") < 0) return text;
  return text.replace(VALUE, (whole, name: string, format: string | undefined) => {
    const v = values?.[name];
    if (v === undefined) return whole;
    return format === "number" && typeof v === "number" ? formatNumber(v) : String(v);
  });
}

/**
 * The words of `key` in the language shown, with `values` in their
 * `{{names}}`; with `count`, the plural form it takes. Markup in the words
 * (`<address>…</address>`, for {@link Trans}) is left out.
 */
export function t<K extends Key>(key: K, ...rest: ValuesFor<K>): string {
  const values = rest[0];
  const count = values && typeof values.count === "number" ? values.count : undefined;
  const text = template(shown.value, key, count);
  if (text === undefined) return key;
  // The markup goes before the values come in: a value is written as it is.
  return fill(text.indexOf("<") >= 0 ? text.replace(TAG, "") : text, values);
}

/**
 * Words with markup in them: each `<name>…</name>` of the key's words is
 * drawn by `tags[name]`, given the words inside it (values filled in); the
 * rest is text. "or go to <address>{{address}}</address>", with
 * `tags={{ address: (text) => <span class="link-address">{text}</span> }}`.
 * Values are filled in after the markup is read, so a value never makes any.
 */
export function Trans<K extends Key>(p: { k: K; values?: Values; tags: Record<string, (text: string) => ComponentChildren> }): VNode {
  const values = p.values;
  const count = values && typeof values.count === "number" ? values.count : undefined;
  const text = template(shown.value, p.k, count) ?? p.k;
  const out: ComponentChildren[] = [];
  let at = 0;
  let open: { name: string; from: number } | null = null;
  // Its own, as a tag drawn may call t(), which uses TAG.
  const tags = new RegExp(TAG.source, "g");
  for (let m = tags.exec(text); m; m = tags.exec(text)) {
    const closing = m[1] === "/";
    const name = m[2]!;
    if (!open && !closing) {
      if (m.index > at) out.push(fill(text.slice(at, m.index), values));
      open = { name, from: m.index + m[0].length };
    } else if (open && closing && name === open.name) {
      const inner = fill(text.slice(open.from, m.index), values);
      const draw = p.tags[name];
      out.push(draw ? draw(inner) : inner);
      open = null;
    } else continue;
    at = m.index + m[0].length;
  }
  if (at < text.length) out.push(fill(text.slice(at), values));
  return createElement(Fragment, null, ...out);
}
