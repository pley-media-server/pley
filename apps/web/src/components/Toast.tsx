import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { CircleAlert, CircleCheck, Info, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/cn";

type ToastTone = "success" | "error" | "info";

interface Toast {
  id: number;
  tone: ToastTone;
  message: ReactNode;
}

interface ToastOptions {
  /** How long it stays, in ms (errors 6 s, others 3.5 s). */
  duration?: number;
}

interface ToastApi {
  success: (message: ReactNode, options?: ToastOptions) => void;
  error: (message: ReactNode, options?: ToastOptions) => void;
  info: (message: ReactNode, options?: ToastOptions) => void;
}

const ToastContext = createContext<ToastApi | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation("components");
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(1);

  const dismiss = useCallback((id: number) => {
    setToasts((list) => list.filter((x) => x.id !== id));
  }, []);

  const push = useCallback(
    (tone: ToastTone, message: ReactNode, options?: ToastOptions) => {
      const id = nextId.current++;
      setToasts((list) => [...list.slice(-3), { id, tone, message }]);
      window.setTimeout(() => dismiss(id), options?.duration ?? (tone === "error" ? 6000 : 3500));
    },
    [dismiss],
  );

  const api = useMemo<ToastApi>(
    () => ({
      success: (m, o) => push("success", m, o),
      error: (m, o) => push("error", m, o),
      info: (m, o) => push("info", m, o),
    }),
    [push],
  );

  return (
    <ToastContext.Provider value={api}>
      {children}
      {createPortal(
        <div className="pointer-events-none fixed inset-x-0 bottom-4 z-[60] flex flex-col items-center gap-2 px-4 sm:bottom-6 sm:items-end sm:px-6">
          {toasts.map((toast) => (
            <div
              key={toast.id}
              role={toast.tone === "error" ? "alert" : "status"}
              className={cn(
                "pointer-events-auto flex w-full max-w-sm animate-pop-in items-start gap-3 rounded-lg border bg-surface-2 px-4 py-3 text-sm shadow-xl shadow-black/50",
                toast.tone === "success" && "border-success/40",
                toast.tone === "error" && "border-danger/50",
                toast.tone === "info" && "border-border-strong",
              )}
            >
              {toast.tone === "success" ? (
                <CircleCheck className="mt-0.5 size-4 shrink-0 text-success" />
              ) : toast.tone === "error" ? (
                <CircleAlert className="mt-0.5 size-4 shrink-0 text-danger" />
              ) : (
                <Info className="mt-0.5 size-4 shrink-0 text-info" />
              )}
              <div className="min-w-0 flex-1 text-fg">{toast.message}</div>
              <button
                type="button"
                aria-label={t("toast.dismiss")}
                className="-mr-1 rounded p-0.5 text-subtle hover:text-fg"
                onClick={() => dismiss(toast.id)}
              >
                <X className="size-4" />
              </button>
            </div>
          ))}
        </div>,
        document.body,
      )}
    </ToastContext.Provider>
  );
}

export function useToast(): ToastApi {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error("useToast must be used inside <ToastProvider>");
  return ctx;
}
