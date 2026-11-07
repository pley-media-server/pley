import { useState } from "react";
import { Check, Clapperboard, Music, Tv } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLibraries, useRatingSystems } from "@/api/hooks";
import type { Id, LibraryKind } from "@/api/types";
import {
  AGE_SYSTEM,
  ageIn,
  ageLevels,
  BIRTHDAY_LIMIT_ENDS,
  birthdayLimit,
  birthdayLimitEnd,
  birthMonthOf,
  monthNames,
  type LimitMode,
  limitMode,
  parseBirthMonth,
  ratingSystem,
  shownSystem,
  systemChoices,
  toggleId,
} from "@/lib/access";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { inBoth } from "@/lib/labels";
import { LabelPicker } from "./LabelPicker";
import { Button, Field, Select } from "./ui";

export interface AccessValue {
  all_libraries: boolean;
  library_ids: Id[];
  max_age: number | null;
  /** The rating system the limit is picked in; `null`: the metadata region's. */
  max_age_system: string | null;
  /** "2015-03": the limit is their age, from this birthday (accounts only, not invitations). */
  birth_month?: string | null;
  /** Labels whose titles show even above the age limit. */
  show_labels: string[];
  /** Labels whose titles never show, whatever their rating. */
  hide_labels: string[];
}

/** Ages a limit can be set to: 21 is the oldest any rating is for. */
const AGES = Array.from({ length: BIRTHDAY_LIMIT_ENDS + 1 }, (_, age) => age);

const KIND_ICON: Record<LibraryKind, typeof Tv> = { movies: Clapperboard, shows: Tv, music: Music };

/**
 * Library access (all, or a chosen set), an age limit, picked as a rating of
 * a rating system, and its exceptions by label.
 */
export function AccessFields({
  value,
  onChange,
  idPrefix,
  birthday = false,
}: {
  value: AccessValue;
  onChange: (v: AccessValue) => void;
  idPrefix: string;
  /** Offer "By birthday", a limit that follows their age. */
  birthday?: boolean;
}) {
  const { t } = useTranslation(["components", "common"]);
  const libs = useLibraries();
  const systems = useRatingSystems().data;
  const mode = limitMode(systems, value);
  const country = ratingSystem(systems, value.max_age_system);
  const levels = country ? ageLevels(systems, country) : [];
  const shown = ageLevels(systems, shownSystem(systems, value.max_age_system, value.max_age)).find(
    (l) => l.age === value.max_age,
  );
  const born = value.birth_month ? parseBirthMonth(value.birth_month) : null;
  const formatAge = (age: number) => age.toLocaleString(currentLocale());
  const limitHint =
    value.max_age == null
      ? born
        ? t("accessFields.grownUpHint", { age: BIRTHDAY_LIMIT_ENDS })
        : t("accessFields.noLimitHint")
      : shown?.rating
        ? t(value.max_age === 0 ? "accessFields.ratingHintAllAges" : "accessFields.ratingHint", {
            rating: shown.rating,
            age: formatAge(value.max_age),
          })
        : t(value.max_age === 0 ? "accessFields.ageHintAllAges" : "accessFields.ageHint", { age: formatAge(value.max_age) });
  const hint =
    born && value.birth_month && value.max_age != null
      ? `${limitHint} ${t("accessFields.birthdayHint", { month: monthNames()[born.month - 1], end: birthdayLimitEnd(value.birth_month) })}`
      : limitHint;
  const thisYear = new Date().getFullYear();
  const years = Array.from({ length: BIRTHDAY_LIMIT_ENDS + 1 }, (_, i) => thisYear - i);
  if (born && !years.includes(born.year)) years.push(born.year);
  const set = (patch: Partial<AccessValue>) => onChange({ ...value, ...patch });
  const both = inBoth(value.show_labels, value.hide_labels);
  // Folded away until asked for: most profiles have none.
  const [exceptionsOpen, setExceptionsOpen] = useState(false);
  const showExceptions = exceptionsOpen || both.length > 0;
  const ruleCount = (mode !== "none" ? value.show_labels.length : 0) + value.hide_labels.length;
  const setBirthMonth = (birthMonth: string) => set({ birth_month: birthMonth, max_age: birthdayLimit(birthMonth) });
  // A limit that isn't a birthday's clears it (for accounts; invitations have none).
  const fixed = birthday ? { birth_month: null } : {};
  /** Each way keeps the limit as near as it can, and starts strict when there was none. */
  const setMode = (next: LimitMode) => {
    const now = new Date();
    if (next === "none") return set({ ...fixed, max_age: null });
    if (next === "age") return set({ ...fixed, max_age_system: AGE_SYSTEM, max_age: value.max_age ?? 0 });
    if (next === "birthday") {
      // Born this month, as many years ago as the limit was: the limit stays.
      return setBirthMonth(birthMonthOf(now.getFullYear() - (value.max_age ?? 10), now.getMonth() + 1));
    }
    if (!country) return;
    // The region's ratings stay "the region's" (null) unless another country is picked.
    const system = value.max_age_system == null && country === systems?.region ? null : country;
    set({ ...fixed, max_age_system: system, max_age: ageIn(systems, country, value.max_age ?? 0) });
  };

  return (
    <div className="space-y-4">
      <fieldset className="space-y-2">
        <legend className="mb-1.5 text-sm font-medium">{t("accessFields.libraries")}</legend>
        <div className="grid grid-cols-2 gap-1 rounded-md bg-surface-2 p-1 text-sm">
          {[
            { all: true, label: t("accessFields.allLibraries") },
            { all: false, label: t("accessFields.onlySome") },
          ].map((o) => (
            <button
              key={String(o.all)}
              type="button"
              aria-pressed={value.all_libraries === o.all}
              onClick={() => set({ all_libraries: o.all })}
              className={cn(
                "h-8 rounded font-medium transition-colors",
                value.all_libraries === o.all ? "bg-surface-3 text-fg shadow-sm" : "text-muted hover:text-fg",
              )}
            >
              {o.label}
            </button>
          ))}
        </div>
        {value.all_libraries ? (
          <p className="text-xs text-subtle">{t("accessFields.includesLater")}</p>
        ) : (
          <div className="max-h-52 space-y-1 overflow-y-auto rounded-md border border-border p-1.5">
            {(libs.data ?? []).map((lib) => {
              const checked = value.library_ids.includes(lib.id);
              const Icon = KIND_ICON[lib.kind];
              return (
                <label
                  key={lib.id}
                  htmlFor={`${idPrefix}-lib-${lib.id}`}
                  className="flex cursor-pointer items-center gap-3 rounded px-2 py-1.5 text-sm hover:bg-white/5"
                >
                  <input
                    id={`${idPrefix}-lib-${lib.id}`}
                    type="checkbox"
                    checked={checked}
                    onChange={() => set({ library_ids: toggleId(value.library_ids, lib.id) })}
                    className="peer sr-only"
                  />
                  <span
                    className={cn(
                      "flex size-4 shrink-0 items-center justify-center rounded border transition-colors peer-focus-visible:ring-2 peer-focus-visible:ring-accent",
                      checked ? "border-accent bg-accent text-accent-fg" : "border-border-strong",
                    )}
                  >
                    {checked ? <Check className="size-3" strokeWidth={3} /> : null}
                  </span>
                  <Icon className="size-4 text-subtle" />
                  <span className="truncate">{lib.name}</span>
                </label>
              );
            })}
            {libs.data && libs.data.length === 0 ? <p className="px-2 py-1.5 text-sm text-subtle">{t("accessFields.noLibraries")}</p> : null}
          </div>
        )}
      </fieldset>

      <div className="space-y-2">
        <Field label={t("accessFields.ageLimit")} htmlFor={`${idPrefix}-age-limit`}>
          <Select
            id={`${idPrefix}-age-limit`}
            className="h-10 w-full"
            value={mode}
            onChange={(e) => setMode(e.target.value as LimitMode)}
          >
            <option value="none">{t("accessFields.mode.none")}</option>
            <option value="rating" disabled={!country}>
              {t("accessFields.mode.rating")}
            </option>
            <option value="age">{t("accessFields.mode.age")}</option>
            {birthday ? <option value="birthday">{t("accessFields.mode.birthday")}</option> : null}
          </Select>
        </Field>
        {mode === "rating" && country ? (
          <div className="grid grid-cols-2 gap-2">
            <Field label={t("accessFields.ratingSystem")} htmlFor={`${idPrefix}-age-system`}>
              <Select
                id={`${idPrefix}-age-system`}
                className="h-10 w-full"
                value={country}
                onChange={(e) => {
                  const next = e.target.value;
                  set({ max_age_system: next, max_age: ageIn(systems, next, value.max_age) });
                }}
              >
                {systemChoices(systems).map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label={t("accessFields.rating")} htmlFor={`${idPrefix}-age`}>
              <Select
                id={`${idPrefix}-age`}
                className="h-10 w-full"
                value={value.max_age ?? ""}
                onChange={(e) => set({ max_age: Number(e.target.value) })}
              >
                {levels.map((l) => (
                  <option key={l.age} value={l.age}>
                    {t("accessFields.ratingOption", { rating: l.rating, age: formatAge(l.age) })}
                  </option>
                ))}
              </Select>
            </Field>
          </div>
        ) : null}
        {mode === "age" ? (
          <Field label={t("accessFields.age")} htmlFor={`${idPrefix}-age`}>
            <Select
              id={`${idPrefix}-age`}
              className="h-10 w-full"
              value={value.max_age ?? ""}
              // Kept as an age, so that one a rating also has (12 in the UK)
              // doesn't turn into that rating.
              onChange={(e) => set({ max_age_system: AGE_SYSTEM, max_age: Number(e.target.value) })}
            >
              {AGES.map((age) => (
                <option key={age} value={age}>
                  {formatAge(age)}
                </option>
              ))}
            </Select>
          </Field>
        ) : null}
        {mode === "birthday" && born ? (
          <div className="grid grid-cols-2 gap-2">
            <Field label={t("accessFields.birthMonth")} htmlFor={`${idPrefix}-birth-month`}>
              <Select
                id={`${idPrefix}-birth-month`}
                className="h-10 w-full"
                value={born.month}
                onChange={(e) => setBirthMonth(birthMonthOf(born.year, Number(e.target.value)))}
              >
                {monthNames().map((name, i) => (
                  <option key={name} value={i + 1}>
                    {name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label={t("accessFields.birthYear")} htmlFor={`${idPrefix}-birth-year`}>
              <Select
                id={`${idPrefix}-birth-year`}
                className="h-10 w-full"
                value={born.year}
                onChange={(e) => setBirthMonth(birthMonthOf(Number(e.target.value), born.month))}
              >
                {years.map((year) => (
                  <option key={year} value={year}>
                    {year}
                  </option>
                ))}
              </Select>
            </Field>
          </div>
        ) : null}
        <p className="text-xs text-subtle">{hint}</p>
      </div>

      <fieldset className="space-y-3">
        <legend className="mb-1.5 text-sm font-medium">{t("accessFields.exceptions")}</legend>
        {showExceptions ? null : (
          <div className="flex items-center justify-between gap-3 rounded-md border border-border bg-surface-2 px-3 py-2">
            <span className="text-sm text-muted">
              {ruleCount === 0
                ? t("accessFields.exceptionsNone")
                : [
                    mode !== "none" && value.show_labels.length > 0
                      ? t("accessFields.exceptionsShown", { count: value.show_labels.length })
                      : null,
                    value.hide_labels.length > 0 ? t("accessFields.exceptionsHidden", { count: value.hide_labels.length }) : null,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
            </span>
            <Button variant="outline" size="sm" onClick={() => setExceptionsOpen(true)}>
              {ruleCount === 0 ? t("accessFields.exceptionsAdd") : t("accessFields.exceptionsChange")}
            </Button>
          </div>
        )}
        {showExceptions ? (
          <>
            <p className="-mt-1 text-xs text-subtle">{t("accessFields.exceptionsHint")}</p>
            {/* Only an age limit hides what these would show; kept as they are while there's none. */}
            {mode !== "none" ? (
              <Field label={t("accessFields.showLabels")} htmlFor={`${idPrefix}-show-labels`} hint={t("accessFields.showLabelsHint")}>
                <LabelPicker
                  id={`${idPrefix}-show-labels`}
                  value={value.show_labels}
                  onChange={(show_labels) => set({ show_labels })}
                  removeLabel={(label) => t("accessFields.removeLabel", { label })}
                />
              </Field>
            ) : null}
            <Field
              label={t("accessFields.hideLabels")}
              htmlFor={`${idPrefix}-hide-labels`}
              hint={t("accessFields.hideLabelsHint")}
              error={both.length > 0 ? t("accessFields.inBoth", { labels: both.join(", ") }) : null}
            >
              <LabelPicker
                id={`${idPrefix}-hide-labels`}
                value={value.hide_labels}
                onChange={(hide_labels) => set({ hide_labels })}
                removeLabel={(label) => t("accessFields.removeLabel", { label })}
              />
            </Field>
          </>
        ) : null}
      </fieldset>
    </div>
  );
}
