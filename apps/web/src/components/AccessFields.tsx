import { Check, Clapperboard, Music, Tv } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLibraries } from "@/api/hooks";
import type { Id, LibraryKind } from "@/api/types";
import { AGE_OPTIONS, ageToSelect, selectToAge, toggleId } from "@/lib/access";
import { cn } from "@/lib/cn";
import { Field, Select } from "./ui";

export interface AccessValue {
  all_libraries: boolean;
  library_ids: Id[];
  max_age: number | null;
}

const KIND_ICON: Record<LibraryKind, typeof Tv> = { movies: Clapperboard, shows: Tv, music: Music };

/** Library access (all, or a chosen set) and an age limit. */
export function AccessFields({
  value,
  onChange,
  idPrefix,
}: {
  value: AccessValue;
  onChange: (v: AccessValue) => void;
  idPrefix: string;
}) {
  const { t } = useTranslation("components");
  const libs = useLibraries();
  const set = (patch: Partial<AccessValue>) => onChange({ ...value, ...patch });

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

      <Field
        label={t("accessFields.ageLimit")}
        htmlFor={`${idPrefix}-age`}
        hint={t("accessFields.ageLimitHint")}
      >
        <Select
          id={`${idPrefix}-age`}
          className="h-10 w-full"
          value={ageToSelect(value.max_age)}
          onChange={(e) => set({ max_age: selectToAge(e.target.value) })}
        >
          {AGE_OPTIONS.map((o) => (
            <option key={o.label} value={ageToSelect(o.value)}>
              {o.label}
            </option>
          ))}
        </Select>
      </Field>
    </div>
  );
}
