import { useEffect, useEffectEvent, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { cn } from "@/lib/cn";
import { useDismiss } from "@/lib/hooks";

/**
 * Small dropdown menu anchored to a trigger. Opened, its first item has
 * focus; the arrow keys, Home and End move through the items, Escape goes
 * back to the trigger and Tab leaves it closed. The keys it takes go no
 * further: a page's own shortcuts (a player's) leave them alone.
 */
export function Menu({
  trigger,
  children,
  align = "right",
  placement = "below",
  label,
  onOpenChange,
  className,
}: {
  trigger: (props: { open: boolean; toggle: () => void }) => ReactNode;
  children: (close: () => void) => ReactNode;
  align?: "left" | "right";
  /** Above its trigger where there is no room below (a dialog's footer). */
  placement?: "below" | "above";
  /** What the menu is, for screen readers. */
  label?: string;
  /** Hears it open and close: a page whose controls hide keeps them while it is open. */
  onOpenChange?: (open: boolean) => void;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  useDismiss(ref, open, () => setOpen(false));
  // Gone while open (what it offered went) is closed too.
  const opened = useEffectEvent((now: boolean) => onOpenChange?.(now));
  useEffect(() => {
    if (!open) return;
    opened(true);
    return () => opened(false);
  }, [open]);

  const items = () => Array.from(menuRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)') ?? []);
  useEffect(() => {
    if (open) items()[0]?.focus();
  }, [open]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const list = items();
    const at = list.indexOf(document.activeElement as HTMLElement);
    let next: number;
    switch (e.key) {
      case "ArrowDown":
        next = at + 1 < list.length ? at + 1 : 0;
        break;
      case "ArrowUp":
        next = at > 0 ? at - 1 : list.length - 1;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = list.length - 1;
        break;
      case "Escape":
        e.stopPropagation();
        setOpen(false);
        ref.current?.querySelector<HTMLElement>("button, [href], [tabindex]")?.focus();
        return;
      case "Tab":
        setOpen(false);
        return;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
    list[next]?.focus();
  };

  return (
    <div ref={ref} className="relative">
      {trigger({ open, toggle: () => setOpen((o) => !o) })}
      {open ? (
        <div
          ref={menuRef}
          role="menu"
          aria-label={label}
          onKeyDown={onKeyDown}
          className={cn(
            "absolute z-40 min-w-48 animate-pop-in overflow-hidden rounded-lg border border-border bg-surface-2 py-1 shadow-xl shadow-black/50",
            placement === "below" ? "top-full mt-2" : "bottom-full mb-2",
            align === "right" ? "right-0" : "left-0",
            className,
          )}
        >
          {children(() => setOpen(false))}
        </div>
      ) : null}
    </div>
  );
}

export function MenuItem({
  children,
  onClick,
  icon,
  danger,
  disabled,
}: {
  children: ReactNode;
  onClick: () => void;
  icon?: ReactNode;
  danger?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex w-full items-center gap-2.5 px-3.5 py-2 text-left text-sm transition-colors outline-none hover:bg-white/5 focus-visible:bg-white/10 disabled:opacity-50",
        danger ? "text-danger" : "text-fg",
      )}
    >
      {icon ? <span className="text-muted [&>svg]:size-4">{icon}</span> : null}
      {children}
    </button>
  );
}

export function MenuSeparator() {
  return <div className="my-1 h-px bg-border" />;
}
