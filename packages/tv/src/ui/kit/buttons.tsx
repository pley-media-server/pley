import type { ComponentChildren } from "preact";
import { F } from "./focusable";
import { Icon, type IconName } from "./icons";

interface ButtonProps {
  fid?: string;
  initial?: boolean;
  disabled?: boolean;
  onSelect?: () => void;
  onLongSelect?: () => void;
  onFocus?: (el: HTMLElement) => void;
}

/** A pill: an optional icon and a label; Accent when focused, 1.05 (the Android app's TvPillButton). */
export function Pill(p: ButtonProps & { label: string; icon?: IconName }) {
  return (
    <F class="pill t-label" fid={p.fid} initial={p.initial} disabled={p.disabled} label={p.label} onSelect={p.onSelect} onLongSelect={p.onLongSelect} onFocus={p.onFocus}>
      {p.icon && <Icon name={p.icon} size={20} />}
      <span class="one-line">{p.label}</span>
    </F>
  );
}

/**
 * A 40×40 icon button whose label shows under it only while focused; the box keeps room for it
 * (TvRoundButton). `bare`, the square alone, its label only heard (a star that says it all).
 */
export function Round(p: ButtonProps & { label: string; icon: IconName; offArt?: boolean; bare?: boolean }) {
  return (
    <div class={`round-box${p.bare ? " bare" : ""}`}>
      <F
        class={`round${p.offArt ? " off-art" : ""}`}
        fid={p.fid}
        initial={p.initial}
        disabled={p.disabled}
        label={p.label}
        onSelect={p.onSelect}
        onLongSelect={p.onLongSelect}
        onFocus={p.onFocus}
      >
        <Icon name={p.icon} size={20} />
      </F>
      {!p.bare && (
        <div class="round-label t-caption" aria-hidden="true">
          {p.label}
        </div>
      )}
    </div>
  );
}

/**
 * A text button (TvTextButton): fg at 8 % behind its label at rest, so it
 * reads as a button before it has focus; `link` stays bare (an album's
 * artist), `tall` is 48 high (Up Next's Cancel).
 */
export function TextButton(p: ButtonProps & { label: string; color?: string; variant?: "link" | "tall" }) {
  return (
    <F class={`text-btn t-label${p.variant ? ` ${p.variant}` : ""}`} fid={p.fid} initial={p.initial} disabled={p.disabled} label={p.label} onSelect={p.onSelect} onFocus={p.onFocus}>
      <span style={p.color ? `color:${p.color}` : undefined}>{p.label}</span>
    </F>
  );
}

/** A chip: Surface3, a leading icon or a check when selected (the library's TvChip). */
export function Chip(p: ButtonProps & { label: string; icon?: IconName; selected?: boolean; maxWidth?: number }) {
  return (
    <F class="chip t-label" fid={p.fid} initial={p.initial} disabled={p.disabled} label={p.label} onSelect={p.onSelect} onFocus={p.onFocus}>
      {p.selected ? <Icon name="check" size={20} class="chip-check" /> : p.icon ? <Icon name={p.icon} size={20} /> : null}
      <span class="one-line" style={p.maxWidth ? `max-width:${p.maxWidth}px` : undefined}>
        {p.label}
      </span>
    </F>
  );
}

/** A tab: the shown one on Surface3, the others plain Muted text. */
export function Tab(p: ButtonProps & { label: string; shown: boolean }) {
  return (
    <F class={`tab t-label${p.shown ? " shown" : ""}`} fid={p.fid} initial={p.initial} label={p.label} onSelect={p.onSelect} onFocus={p.onFocus}>
      <span class="one-line">{p.label}</span>
    </F>
  );
}

/** A row of a side panel: an optional icon, a label and a second line, a check on the choice in force. */
export function PanelRow(
  p: ButtonProps & {
    label: ComponentChildren;
    sub?: string | null;
    icon?: IconName;
    checked?: boolean;
    trailing?: ComponentChildren;
    class?: string;
    /** In DangerText (Remove, Delete): what can't be undone. */
    danger?: boolean;
    onKey?: (key: string, e: { repeat: boolean }) => boolean;
  },
) {
  return (
    <F
      class={`panel-row ${p.class ?? ""}${p.danger ? " danger" : ""}`}
      fid={p.fid}
      initial={p.initial}
      disabled={p.disabled}
      label={typeof p.label === "string" ? p.label : undefined}
      onSelect={p.onSelect}
      onFocus={p.onFocus}
      onKey={p.onKey}
    >
      {p.icon && <Icon name={p.icon} size={20} class="lead" />}
      <div class="grow">
        <div class="t-body lines-2">{p.label}</div>
        {p.sub && <div class="sub t-caption one-line">{p.sub}</div>}
      </div>
      {p.trailing}
      {p.checked && <Icon name="check" size={20} class="check" />}
    </F>
  );
}
