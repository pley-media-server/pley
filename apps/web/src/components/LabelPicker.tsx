import { useState } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { useLabels } from "@/api/hooks";
import { addLabel } from "@/lib/labels";
import { TextInput } from "./ui";

/**
 * A list of labels: those chosen, each with a button that removes it, and a
 * field that adds one, suggesting the labels titles carry (administrators
 * only) while still taking a name no title has yet.
 */
export function LabelPicker({
  id,
  value,
  onChange,
  removeLabel,
}: {
  id: string;
  value: string[];
  onChange: (labels: string[]) => void;
  /** What the remove button of `label` says, for screen readers. */
  removeLabel: (label: string) => string;
}) {
  const { t } = useTranslation("components");
  const known = useLabels().data ?? [];
  const [text, setText] = useState("");
  const add = () => {
    onChange(addLabel(value, text));
    setText("");
  };
  const choices = known.filter((k) => !value.some((v) => v.toLowerCase() === k.label.toLowerCase()));

  return (
    <div className="space-y-2">
      {value.length > 0 ? (
        <ul className="flex flex-wrap gap-1.5">
          {value.map((label) => (
            <li key={label} className="inline-flex items-center gap-1 rounded bg-white/10 py-0.5 pl-2 pr-0.5 text-xs font-semibold text-fg/90">
              {label}
              <button
                type="button"
                aria-label={removeLabel(label)}
                onClick={() => onChange(value.filter((v) => v !== label))}
                className="rounded p-0.5 text-subtle hover:bg-white/10 hover:text-fg"
              >
                <X className="size-3" />
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      <TextInput
        id={id}
        value={text}
        list={`${id}-known`}
        placeholder={t("labelPicker.placeholder")}
        maxLength={120}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => text.trim() && add()}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            add();
          }
        }}
      />
      <datalist id={`${id}-known`}>
        {choices.map((k) => (
          <option key={k.label} value={k.label}>
            {t("labelPicker.titles", { count: k.items })}
          </option>
        ))}
      </datalist>
    </div>
  );
}
