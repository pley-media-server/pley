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

/** The systems to choose from, by name in the language shown, then Age. */
export function systemChoices(systems: RatingSystems | undefined): { value: string; label: string }[] {
  const collator = new Intl.Collator(currentLocale());
  const countries = (systems?.systems ?? [])
    .map((s) => ({ value: s.country, label: systemName(s) }))
    .sort((a, b) => collator.compare(a.label, b.label));
  return [...countries, { value: AGE_SYSTEM, label: systemName(AGE_SYSTEM) }];
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

/** "Up to PG-13", "Up to age 8", or null without a limit. */
export function ageBadge(
  systems: RatingSystems | undefined,
  stored: string | null | undefined,
  maxAge: number | null | undefined,
): string | null {
  if (maxAge == null) return null;
  const rating = limitRating(systems, stored, maxAge);
  return rating ? t("access.upTo", { rating }) : t("access.upToAge", { age: maxAge });
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

/** Encode the age select's value (`""` = no limit). */
export function ageToSelect(age: number | null): string {
  return age == null ? "" : String(age);
}

export function selectToAge(value: string): number | null {
  return value === "" ? null : Number(value);
}

export function toggleId(ids: Id[], id: Id): Id[] {
  return ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id];
}
