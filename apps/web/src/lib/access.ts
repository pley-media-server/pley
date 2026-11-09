import type { Id, Library, RatingSystem, RatingSystems, User } from "@/api/types";
import { currentLocale, t } from "@/i18n";

/** The rating system of plain ages, 0 to 21. */
export const AGE_SYSTEM = "age";

const AGES = Array.from({ length: 22 }, (_, age) => age);

/** One choice of a system: an age and the ratings for it ("PG, TV-PG"). */
export interface AgeLevel {
  age: number;
  /** `null` in the Age system. */
  rating: string | null;
}

/**
 * The ages a system's ratings stand for, youngest first, with their names.
 * A name that's at two ages (Canada's PG: 10 for a film, 8 on TV) says
 * which.
 */
export function ageLevels(systems: RatingSystems | undefined, system: string): AgeLevel[] {
  const found = systems?.systems.find((s) => s.country === system);
  if (!found) return AGES.map((age) => ({ age, rating: null }));
  const agesOf = new Map<string, Set<number>>();
  for (const r of found.ratings) agesOf.set(r.name, (agesOf.get(r.name) ?? new Set()).add(r.age));
  const byAge = new Map<number, string[]>();
  for (const r of found.ratings) {
    const name =
      r.kind && (agesOf.get(r.name)?.size ?? 0) > 1 ? t(`access.ratingFor.${r.kind}`, { rating: r.name }) : r.name;
    const names = byAge.get(r.age) ?? [];
    if (!names.includes(name)) names.push(name);
    byAge.set(r.age, names);
  }
  return [...byAge.entries()].sort(([a], [b]) => a - b).map(([age, names]) => ({ age, rating: names.join(", ") }));
}

/** A system's name in the language shown: "United Kingdom", "Age". */
export function systemName(system: RatingSystem | string): string {
  const code = typeof system === "string" ? system : system.country;
  if (code === AGE_SYSTEM) return t("access.ageSystem");
  try {
    const name = new Intl.DisplayNames([currentLocale()], { type: "region" }).of(code);
    if (name && name !== code) return name;
  } catch {
    // An old browser: the server's English name.
  }
  return typeof system === "string" ? code : system.name;
}

/** The countries whose ratings a limit can be picked in, by name in the language shown. */
export function systemChoices(systems: RatingSystems | undefined): { value: string; label: string }[] {
  const collator = new Intl.Collator(currentLocale());
  return (systems?.systems ?? [])
    .map((s) => ({ value: s.country, label: systemName(s) }))
    .sort((a, b) => collator.compare(a.label, b.label));
}

/** The country of a metadata language ("GB" for "en-GB"), the US's without one, as the server reads it. */
export function regionOf(language: string): string {
  return language.split(/[-_]/)[1]?.toUpperCase() || "US";
}

/** The countries a server's ratings can be read as: those whose ratings Pley knows, and `current`. */
export function countryChoices(systems: RatingSystems | undefined, current: string): { value: string; label: string }[] {
  const choices = systemChoices(systems);
  if (current && !choices.some((c) => c.value === current)) return [{ value: current, label: systemName(current) }, ...choices];
  return choices;
}

/** How an age limit is set: what the age limit select shows. */
export type LimitMode = "none" | "rating" | "age" | "birthday";

export function limitMode(
  systems: RatingSystems | undefined,
  value: { max_age: number | null; max_age_system: string | null; birth_month?: string | null },
): LimitMode {
  if (value.birth_month) return "birthday";
  if (value.max_age == null) return "none";
  return shownSystem(systems, value.max_age_system, value.max_age) === AGE_SYSTEM ? "age" : "rating";
}

/** The country to pick a rating in: the one stored, else the metadata region, else the US. */
export function ratingSystem(systems: RatingSystems | undefined, stored: string | null): string | null {
  const countries = systems?.systems ?? [];
  const has = (code: string | null) => !!code && countries.some((s) => s.country === code);
  if (has(stored)) return stored;
  if (systems && has(systems.region)) return systems.region;
  // A region without ratings here reads ratings without a country as the
  // US's, as the server does.
  if (has("US")) return "US";
  return countries[0]?.country ?? null;
}

/**
 * The country written before a rating ("GB:15", "gb/12A", "USA:R") when
 * it's one with ratings here: the server reads the rating as that
 * country's. Portugal's "M/12" is a rating, not a country.
 */
export function writtenCountry(systems: RatingSystems | undefined, rating: string): string | null {
  const cut = rating.search(/[:/]/);
  if (cut < 0) return null;
  const before = rating.slice(0, cut).trim().toUpperCase();
  if (before === "USA") return "US";
  return systems?.systems.some((s) => s.country === before) ? before : null;
}

/**
 * The system a limit shows in: the one stored, else the metadata region's,
 * else Age; and Age when the system has no rating for the limit (8 in the
 * US), so that it never reads as no limit.
 */
export function shownSystem(systems: RatingSystems | undefined, stored: string | null | undefined, maxAge: number | null): string {
  const has = (code: string) => code === AGE_SYSTEM || !!systems?.systems.some((s) => s.country === code);
  const system = stored && has(stored) ? stored : systems && has(systems.region) ? systems.region : AGE_SYSTEM;
  if (maxAge == null || system === AGE_SYSTEM) return system;
  return ageLevels(systems, system).some((l) => l.age === maxAge) ? system : AGE_SYSTEM;
}

/** The limit to keep when the system changes: the new system's highest at or below it, else its lowest. */
export function ageIn(systems: RatingSystems | undefined, system: string, maxAge: number | null): number | null {
  if (maxAge == null) return null;
  const ages = ageLevels(systems, system).map((l) => l.age);
  return ages.filter((a) => a <= maxAge).at(-1) ?? ages[0] ?? maxAge;
}

/** The ratings a limit is shown as ("PG-13"); `null` in the Age system. */
function limitRating(systems: RatingSystems | undefined, stored: string | null | undefined, maxAge: number): string | null {
  const system = shownSystem(systems, stored, maxAge);
  return ageLevels(systems, system).find((l) => l.age === maxAge)?.rating ?? null;
}

/** "PG-13", "Age 8", or "No limit". */
export function limitLabel(systems: RatingSystems | undefined, stored: string | null | undefined, maxAge: number | null | undefined): string {
  if (maxAge == null) return t("access.noLimit");
  return limitRating(systems, stored, maxAge) ?? t("access.age", { age: maxAge });
}

/** "Up to PG-13", "Up to age 8", "Up to PG-13 (age 13, by birthday)", or null without a limit. */
export function ageBadge(
  systems: RatingSystems | undefined,
  stored: string | null | undefined,
  maxAge: number | null | undefined,
  birthMonth?: string | null,
): string | null {
  if (maxAge == null) return null;
  const rating = limitRating(systems, stored, maxAge);
  if (birthMonth) {
    return rating ? t("access.upToByBirthday", { rating, age: maxAge }) : t("access.upToAgeByBirthday", { age: maxAge });
  }
  return rating ? t("access.upTo", { rating }) : t("access.upToAge", { age: maxAge });
}

/** The age a limit that follows a birthday ends at, as the server's. */
export const BIRTHDAY_LIMIT_ENDS = 21;

/** `"2015-03"` → year and month (1 to 12). */
export function parseBirthMonth(birthMonth: string): { year: number; month: number } | null {
  const match = /^(\d{4})-(\d{2})$/.exec(birthMonth);
  if (!match) return null;
  const [year, month] = [Number(match[1]), Number(match[2])];
  return month >= 1 && month <= 12 ? { year, month } : null;
}

export function birthMonthOf(year: number, month: number): string {
  return `${String(year).padStart(4, "0")}-${String(month).padStart(2, "0")}`;
}

/**
 * The limit of someone born in `birthMonth` (their age, which goes up on
 * the first of the month they were born in), as the server works it out;
 * null from 21.
 */
export function birthdayLimit(birthMonth: string, now: Date = new Date()): number | null {
  const born = parseBirthMonth(birthMonth);
  if (!born) return null;
  const age = Math.max(0, now.getFullYear() - born.year - (now.getMonth() + 1 < born.month ? 1 : 0));
  return age < BIRTHDAY_LIMIT_ENDS ? age : null;
}

/** "March 2036": when a limit that follows a birthday ends. */
export function birthdayLimitEnd(birthMonth: string): string {
  const born = parseBirthMonth(birthMonth);
  if (!born) return "";
  const end = new Date(born.year + BIRTHDAY_LIMIT_ENDS, born.month - 1, 1);
  return new Intl.DateTimeFormat(currentLocale(), { month: "long", year: "numeric" }).format(end);
}

/** The months' names in the language shown, January first. */
export function monthNames(): string[] {
  const format = new Intl.DateTimeFormat(currentLocale(), { month: "long" });
  return Array.from({ length: 12 }, (_, i) => format.format(new Date(2000, i, 1)));
}

/** "All libraries", "Movies", "2 libraries". */
export function librariesLabel(
  access: Pick<User, "all_libraries" | "library_ids">,
  libraries: Library[] | undefined,
): string {
  if (access.all_libraries) return t("access.allLibraries");
  const ids = access.library_ids;
  if (ids.length === 0) return t("access.noLibraries");
  if (ids.length === 1) {
    const name = libraries?.find((l) => l.id === ids[0])?.name;
    if (name) return name;
  }
  return t("access.libraries", { count: ids.length });
}

export function toggleId(ids: Id[], id: Id): Id[] {
  return ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id];
}
