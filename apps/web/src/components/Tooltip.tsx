import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { cn } from "@/lib/cn";

/** Where a tip opens: beside its trigger (the narrow sidebar's icons) or under it (the top bar's). */
export type TooltipSide = "right" | "bottom";

/** What the trigger takes to show and hide its tip. */
export interface TooltipTrigger {
  onPointerEnter?: (e: { currentTarget: HTMLElement; pointerType: string }) => void;
  onPointerLeave?: () => void;
  onFocus?: (e: { currentTarget: HTMLElement }) => void;
  onBlur?: () => void;
  onPointerDownCapture?: () => void;
  onKeyDownCapture?: (e: { key: string }) => void;
}

// The first tip waits a moment, so a pointer passing over doesn't flash names; one opening as another
// closes comes at once, so going along a row of icons reads them without waiting.
const DELAY_MS = 400;
/**
 * For a button the pointer often rests near on its way elsewhere, whose tip would come up unasked. It
 * waits even right after another tip.
 */
export const LONG_DELAY_MS = 1000;
const WARM_MS = 300;
const FADE_MS = 150;
const GAP_PX = 10;
const EDGE_PX = 8;
let lastOpen = 0;

function byKeyboard(el: HTMLElement): boolean {
  try {
    return el.matches(":focus-visible");
  } catch {
    return true;
  }
}

/**
 * An icon's name, drawn beside it on a mouse's hover or keyboard focus, for
 * icons whose name isn't written. The trigger takes `trigger`'s handlers, and
 * `tip` is rendered beside it. Screen readers already have the name from the
 * trigger's label, so the tip is hidden from them. Escape, a press or a key
 * that acts closes it. No `text`, no tip.
 */
export function useTooltip(
  text: string | undefined,
  { side = "right", delayMs = DELAY_MS }: { side?: TooltipSide; delayMs?: number } = {},
): { trigger: TooltipTrigger; tip: ReactNode } {
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const [open, setOpen] = useState(false);
  const isOpen = useRef(false);
  const target = useRef<HTMLElement | null>(null);
  const showing = useRef<number | undefined>(undefined);
  const going = useRef<number | undefined>(undefined);
  const tipRef = useRef<HTMLDivElement>(null);
  const arrowRef = useRef<HTMLSpanElement>(null);

  const show = useCallback(
    (el: HTMLElement, now: boolean) => {
      window.clearTimeout(showing.current);
      const place = () => {
        window.clearTimeout(going.current);
        target.current = el;
        setAnchor(el.getBoundingClientRect());
        isOpen.current = true;
        setOpen(true);
      };
      if (now || (delayMs <= DELAY_MS && Date.now() - lastOpen < WARM_MS)) place();
      else showing.current = window.setTimeout(place, delayMs);
    },
    [delayMs],
  );

  const hide = useCallback(() => {
    window.clearTimeout(showing.current);
    if (!isOpen.current) return;
    lastOpen = Date.now();
    isOpen.current = false;
    setOpen(false);
    // Gone once it has faded.
    going.current = window.setTimeout(() => setAnchor(null), FADE_MS);
  }, []);

  const drop = useCallback(() => {
    window.clearTimeout(going.current);
    setAnchor(null);
  }, []);

  useEffect(() => {
    if (!text) hide();
    // A fading tip whose words changed (Collapse became Expand) would say them where its trigger was.
    else if (!isOpen.current) drop();
  }, [text, hide, drop]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") hide();
    };
    // Measured again when something moves its trigger: Tab scrolls the rail to the icon it reaches.
    const follow = () => {
      if (target.current?.isConnected) setAnchor(target.current.getBoundingClientRect());
      else hide();
    };
    document.addEventListener("keydown", onKey);
    window.addEventListener("scroll", follow, true);
    window.addEventListener("resize", follow);
    return () => {
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", follow, true);
      window.removeEventListener("resize", follow);
    };
  }, [open, hide]);

  useEffect(
    () => () => {
      window.clearTimeout(showing.current);
      window.clearTimeout(going.current);
    },
    [],
  );

  // Placed before it's painted, from its own size: centred on the trigger, kept on the screen.
  useLayoutEffect(() => {
    const tip = tipRef.current;
    const arrow = arrowRef.current;
    if (!tip || !arrow || !anchor) return;
    if (side === "right") {
      tip.style.left = `${anchor.right + GAP_PX}px`;
      tip.style.top = `${anchor.top + anchor.height / 2 - tip.offsetHeight / 2}px`;
    } else {
      const centre = anchor.left + anchor.width / 2;
      const left = Math.max(EDGE_PX, Math.min(centre - tip.offsetWidth / 2, window.innerWidth - EDGE_PX - tip.offsetWidth));
      tip.style.left = `${left}px`;
      tip.style.top = `${anchor.bottom + GAP_PX}px`;
      arrow.style.left = `${centre - left}px`;
    }
  }, [anchor, side, text]);

  const trigger: TooltipTrigger = text
    ? {
        // A mouse only: a touch has no hover to end it, so its tip would come up after the tap and stay.
        onPointerEnter: (e) => {
          if (e.pointerType === "mouse") show(e.currentTarget, false);
        },
        onPointerLeave: hide,
        onFocus: (e) => {
          if (byKeyboard(e.currentTarget)) show(e.currentTarget, true);
        },
        onBlur: hide,
        onPointerDownCapture: hide,
        onKeyDownCapture: (e) => {
          if (e.key === "Enter" || e.key === " ") hide();
        },
      }
    : {};

  return {
    trigger,
    tip:
      anchor && text
        ? createPortal(
            <div
              ref={tipRef}
              role="tooltip"
              aria-hidden
              data-open={open || undefined}
              className={cn(
                "pointer-events-none fixed left-0 top-0 z-[70] whitespace-nowrap rounded-md border border-border bg-surface-2 px-2.5 py-1.5 text-xs font-medium text-fg shadow-lg shadow-black/40 transition-[opacity,translate] duration-150 ease-out motion-reduce:transition-none",
                side === "right" ? "starting:-translate-x-1" : "starting:-translate-y-1",
                "starting:opacity-0",
                open ? "opacity-100" : side === "right" ? "-translate-x-1 opacity-0" : "-translate-y-1 opacity-0",
              )}
            >
              {text}
              {/* The point towards the trigger, its two outer edges bordered like the tip. */}
              <span
                ref={arrowRef}
                className={cn(
                  "absolute size-2 rotate-45 border-border bg-surface-2",
                  side === "right" ? "-left-1 top-1/2 -mt-1 border-b border-l" : "-top-1 -ml-1 border-l border-t",
                )}
              />
            </div>,
            document.body,
          )
        : null,
  };
}
