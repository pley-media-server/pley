import { useEffect, useEffectEvent, useId, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/cn";
import { Button, IconButton } from "./ui";

export interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  size?: "sm" | "md" | "lg" | "xl";
  /** Prevent closing via backdrop / Escape (e.g. while saving). */
  locked?: boolean;
}

/** Keeps Tab and Shift+Tab inside `container`: past its last control, back to its first. */
export function keepTabInside(e: KeyboardEvent, container: HTMLElement) {
  const focusables = container.querySelectorAll<HTMLElement>(
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
  );
  const first = focusables[0];
  const last = focusables[focusables.length - 1];
  if (!first || !last) return;
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
  }
}

export function Modal({ open, onClose, title, children, footer, size = "md", locked }: ModalProps) {
  const { t } = useTranslation();
  const panelRef = useRef<HTMLDivElement>(null);
  const titleId = useId();
  /** Escape closes the dialog unless it is locked (a request in flight). */
  const onEscape = useEffectEvent((e: KeyboardEvent) => {
    if (locked) return;
    e.stopPropagation();
    onClose();
  });

  useEffect(() => {
    if (!open) return;
    const prevFocus = document.activeElement as HTMLElement | null;
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onEscape(e);
      if (e.key === "Tab" && panelRef.current) keepTabInside(e, panelRef.current);
    };
    document.addEventListener("keydown", onKey, true);
    // Focus the first input (or the panel) on open.
    window.setTimeout(() => {
      const target =
        panelRef.current?.querySelector<HTMLElement>("[data-autofocus]") ??
        panelRef.current?.querySelector<HTMLElement>("input, select, textarea") ??
        panelRef.current;
      target?.focus();
    }, 0);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      document.body.style.overflow = prevOverflow;
      prevFocus?.focus?.();
    };
  }, [open]);

  if (!open) return null;

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-end justify-center p-0 sm:items-center sm:p-6">
      <div
        className="absolute inset-0 animate-fade-in bg-black/70 backdrop-blur-sm"
        onClick={() => {
          if (!locked) onClose();
        }}
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className={cn(
          "relative flex max-h-[92vh] w-full animate-pop-in flex-col overflow-hidden rounded-t-xl border border-border bg-surface shadow-2xl shadow-black/60 outline-none sm:rounded-xl",
          size === "sm" && "sm:max-w-sm",
          size === "md" && "sm:max-w-lg",
          size === "lg" && "sm:max-w-2xl",
          size === "xl" && "sm:max-w-4xl",
        )}
      >
        <div className="flex items-center justify-between gap-4 border-b border-border px-5 py-3.5">
          {/* A question naming a long title wraps, rather than losing its end. */}
          <h2 id={titleId} className="min-w-0 break-words text-base font-semibold">
            {title}
          </h2>
          <IconButton label={t("actions.close")} size="sm" onClick={onClose} disabled={locked}>
            <X className="size-4" />
          </IconButton>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">{children}</div>
        {footer ? <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-3">{footer}</div> : null}
      </div>
    </div>,
    document.body,
  );
}

export function ConfirmModal({
  open,
  onClose,
  onConfirm,
  title,
  message,
  confirmLabel,
  danger,
  loading,
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: string;
  message: ReactNode;
  confirmLabel?: string;
  danger?: boolean;
  loading?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      size="sm"
      locked={loading}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={loading}>
            {t("actions.cancel")}
          </Button>
          <Button variant={danger ? "danger" : "primary"} onClick={onConfirm} loading={loading} data-autofocus>
            {confirmLabel ?? t("actions.confirm")}
          </Button>
        </>
      }
    >
      <div className="text-sm text-muted">{message}</div>
    </Modal>
  );
}
