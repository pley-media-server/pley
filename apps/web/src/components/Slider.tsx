import { useState } from "react";
import { cn } from "@/lib/cn";

export interface SliderProps {
  value: number;
  max: number;
  /** Called once when the user finishes dragging (or on each step for keyboard). */
  onCommit?: (v: number) => void;
  /** Called continuously while dragging. */
  onPreview?: (v: number) => void;
  buffered?: number;
  label: string;
  /** What a screen reader says for a value, where the bare number means little (milliseconds). */
  valueText?: (v: number) => string;
  className?: string;
  step?: number;
}

/** Thin range slider with an accent fill; native <input type=range> underneath for a11y. */
export function Slider({ value, max, onCommit, onPreview, buffered, label, valueText, className, step }: SliderProps) {
  const [drag, setDrag] = useState<number | null>(null);
  const v = drag ?? value;
  const pct = max > 0 ? Math.min(100, Math.max(0, (v / max) * 100)) : 0;
  const bufPct = buffered != null && max > 0 ? Math.min(100, (buffered / max) * 100) : 0;

  const commit = () => {
    if (drag == null) return;
    onCommit?.(drag);
    setDrag(null);
  };

  return (
    <div className={cn("group/slider relative flex h-5 items-center", className)}>
      <div className="relative h-1 w-full overflow-hidden rounded-full bg-white/15 transition-[height] duration-150 group-hover/slider:h-1.5">
        {buffered != null ? <div className="absolute inset-y-0 left-0 bg-white/25" style={{ width: `${bufPct}%` }} /> : null}
        <div className="absolute inset-y-0 left-0 rounded-full bg-accent" style={{ width: `${pct}%` }} />
      </div>
      <div
        className="pointer-events-none absolute top-1/2 size-3 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white opacity-0 shadow transition-opacity group-hover/slider:opacity-100"
        style={{ left: `${pct}%` }}
      />
      <input
        type="range"
        aria-label={label}
        aria-valuetext={valueText?.(v)}
        min={0}
        max={max > 0 ? max : 1}
        step={step ?? "any"}
        value={Math.min(v, max > 0 ? max : 1)}
        onChange={(e) => {
          const n = Number(e.target.value);
          setDrag(n);
          onPreview?.(n);
        }}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
        onTouchEnd={commit}
        className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
      />
    </div>
  );
}
