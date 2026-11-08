import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, X } from "lucide-react";
import { useLabels } from "@/api/hooks";
import { addLabel, labelKey } from "@/lib/labels";
import { cn } from "@/lib/cn";

/** Most suggestions listed at once; typing narrows the rest. */
const MAX_SHOWN = 50;

type Choice = { label: string; items?: number; rules?: number };

/**
 * A list of labels in one field: those chosen as chips, each with a button
 * that removes it, then the text that adds one. As it's typed, a list
 * suggests the labels titles carry (administrators only), or offers to add
 * the text as a new label. Up and down move through the list, Enter or a
 * click adds the one highlighted, Escape closes the list, and Backspace in
 * an empty field removes the last chip.
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
  const inputRef = useRef<HTMLInputElement>(null);
  const listId = useId();
  const [text, setText] = useState("");
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);

  const query = text.trim();
  const lower = query.toLowerCase();
  const chosen = new Set(value.map(labelKey));
  const matches = known
    .filter((k) => !chosen.has(labelKey(k.label)) && k.label.toLowerCase().includes(lower))
    // The label typed exactly, then those it starts, then the rest.
    .sort((a, b) => rank(a.label, lower) - rank(b.label, lower));
  const isNew = query !== "" && !chosen.has(labelKey(query)) && !known.some((k) => labelKey(k.label) === labelKey(query));
  const choices: Choice[] = [...matches.slice(0, MAX_SHOWN), ...(isNew ? [{ label: query }] : [])];
  const shown = open && choices.length > 0;
  const current = Math.min(active, choices.length - 1);
  const optionId = (i: number) => `${listId}-${i}`;

  // An opened list scrolls the dialog it's in until it's all in sight.
  useEffect(() => {
    if (shown) document.getElementById(listId)?.scrollIntoView?.({ block: "nearest" });
  }, [shown, listId]);

  useEffect(() => {
    if (shown) document.getElementById(optionId(current))?.scrollIntoView?.({ block: "nearest" });
    // optionId only joins listId, which never changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shown, current]);

  const add = (label: string) => {
    onChange(addLabel(value, label));
    setText("");
    setActive(0);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    switch (e.key) {
      case "ArrowDown":
      case "ArrowUp": {
        e.preventDefault();
        if (!shown) {
          setOpen(true);
          setActive(0);
        } else if (choices.length > 0) {
          const step = e.key === "ArrowDown" ? 1 : -1;
          setActive((current + step + choices.length) % choices.length);
        }
        break;
      }
      case "Enter":
        // Enter never submits the form around the field.
        e.preventDefault();
        if (shown) add(choices[current]?.label ?? query);
        else if (query) add(query);
        break;
      case "Escape":
        // Modal leaves Escape to an open list, so this closes only the list.
        if (shown) {
          e.preventDefault();
          setOpen(false);
        }
        break;
      case "Backspace":
        if (text === "" && value.length > 0) onChange(value.slice(0, -1));
        break;
    }
  };

  return (
    <div className="relative">
      <div
        className="flex min-h-10 w-full cursor-text flex-wrap items-center gap-1.5 rounded-md border border-border bg-surface-2 px-2 py-1.5 transition-colors focus-within:border-accent"
        onClick={() => inputRef.current?.focus()}
      >
        {value.map((label) => (
          <span
            key={label}
            className="inline-flex items-center gap-1 rounded bg-white/10 py-0.5 pl-2 pr-0.5 text-xs font-semibold text-fg/90"
          >
            {label}
            <button
              type="button"
              aria-label={removeLabel(label)}
              onClick={() => onChange(value.filter((v) => v !== label))}
              className="rounded p-0.5 text-subtle hover:bg-white/10 hover:text-fg"
            >
              <X className="size-3" />
            </button>
          </span>
        ))}
        <input
          ref={inputRef}
          id={id}
          value={text}
          role="combobox"
          aria-expanded={shown}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={shown ? optionId(current) : undefined}
          autoComplete="off"
          placeholder={value.length === 0 ? t("labelPicker.placeholder") : undefined}
          maxLength={120}
          className="h-7 min-w-32 flex-1 bg-transparent px-1 text-sm text-fg placeholder:text-subtle focus:outline-none"
          onChange={(e) => {
            setText(e.target.value);
            setOpen(true);
            setActive(0);
          }}
          onClick={() => setOpen(true)}
          onKeyDown={onKeyDown}
          onBlur={() => {
            setOpen(false);
            // Text left in the field is kept, as if Enter had been pressed.
            if (query) add(query);
          }}
        />
      </div>
      <ul
        id={listId}
        role="listbox"
        aria-label={t("labelPicker.suggestions")}
        hidden={!shown}
        className="absolute z-40 mt-1 max-h-56 w-full animate-pop-in overflow-y-auto overscroll-contain rounded-lg border border-border bg-surface-2 py-1 shadow-xl shadow-black/50"
      >
        {shown
          ? choices.map((choice, i) => (
              <li
                key={choice.items === undefined ? `new:${choice.label}` : choice.label}
                id={optionId(i)}
                role="option"
                aria-selected={i === current}
                // Keeps focus in the field, so its blur doesn't add the text typed.
                onMouseDown={(e) => e.preventDefault()}
                onMouseMove={() => setActive(i)}
                onClick={() => add(choice.label)}
                className={cn(
                  "flex cursor-pointer items-center justify-between gap-3 px-3 py-1.5 text-sm text-fg",
                  i === current && "bg-white/10",
                )}
              >
                {choice.items === undefined ? (
                  <span className="flex min-w-0 items-center gap-2">
                    <Plus className="size-3.5 shrink-0 text-subtle" />
                    <span className="truncate">{t("labelPicker.create", { label: choice.label })}</span>
                  </span>
                ) : (
                  <>
                    <span className="truncate">{choice.label}</span>
                    <span className="shrink-0 text-xs text-subtle">
                      {choice.items ? t("labelPicker.titles", { count: choice.items }) : t("labelPicker.inRules")}
                    </span>
                  </>
                )}
              </li>
            ))
          : null}
      </ul>
    </div>
  );
}

/** Where `label` sorts for the text typed: 0 the same, 1 starting it, 2 holding it. */
function rank(label: string, lower: string): number {
  const l = label.toLowerCase();
  if (l === lower) return 0;
  return l.startsWith(lower) ? 1 : 2;
}
