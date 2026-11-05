import type { Id, Library, User } from "@/api/types";
import { t } from "@/i18n";

/** An age option whose label is read in the language shown, each time. */
function option(value: number | null, label: () => string): { value: number | null; readonly label: string } {
  return {
    value,
    get label() {
      return label();
    },
  };
}

/** Age limits offered for users and invitations (see the server's rating table). */
export const AGE_OPTIONS: { value: number | null; readonly label: string }[] = [
  option(null, () => t("access.noLimit")),
  option(0, () => t("access.littleKids")),
  option(7, () => t("access.kids")),
  option(10, () => t("access.olderKids")),
  option(13, () => t("access.teens")),
  { value: 14, label: "TV-14" },
  { value: 15, label: "15+" },
  option(17, () => t("access.adults")),
];

/** "Up to 13", or null without a limit. */
export function ageBadge(maxAge: number | null | undefined): string | null {
  return maxAge == null ? null : t("access.upTo", { age: maxAge });
}

export function ageOptionLabel(maxAge: number | null | undefined): string {
  return AGE_OPTIONS.find((o) => o.value === (maxAge ?? null))?.label ?? t("access.upTo", { age: maxAge });
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
