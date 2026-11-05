import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { ArrowLeft, Delete, Lock } from "lucide-react";
import { errorMessage, isApiError } from "@/api/client";
import type { User } from "@/api/types";
import { cn } from "@/lib/cn";
import { Avatar } from "./Avatar";
import { Button, Spinner } from "./ui";

const PAD = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "back", "0", "ok"] as const;

/** What a pad says of a PIN not taken: the server explains 403 (wrong, locked) and 429 (too many tries). */
export function pinError(err: unknown): string {
  return isApiError(err, 0) ? "Can't reach the server" : errorMessage(err);
}

/**
 * A profile's PIN, typed on the pad or the keyboard: digits, Backspace,
 * Enter for OK and Escape to cancel, wherever focus is. The keys it takes go
 * no further: a page's own shortcuts (a player's channel numbers) leave
 * them alone while it shows.
 */
export function PinPad({
  target,
  busy,
  locked,
  error,
  onSubmit,
  onCancel,
  prompt = "Enter the profile PIN",
  submitLabel = "Switch",
  cancelLabel = "Back to profiles",
  cancelIcon = <ArrowLeft className="size-4" />,
  small,
}: {
  /** Whose PIN it is, shown above the pad; left out where what is around the pad says it. */
  target?: User;
  busy: boolean;
  /** The PIN is locked after too many wrong ones: the keys stop. */
  locked: boolean;
  error: string | null;
  /** Called with the PIN once it has at least 4 digits. */
  onSubmit: (pin: string) => void;
  onCancel: () => void;
  prompt?: string;
  /** What OK does, for screen readers. */
  submitLabel?: string;
  cancelLabel?: string;
  cancelIcon?: ReactNode;
  /** Smaller keys, for a pad in a menu's place rather than a page of its own. */
  small?: boolean;
}) {
  const [pin, setPinState] = useState("");
  // Keys can arrive faster than renders (quick typing, password managers):
  // the ref always holds what was typed so far.
  const pinRef = useRef("");
  const setPin = useCallback((next: string) => {
    pinRef.current = next;
    setPinState(next);
  }, []);

  const press = useCallback(
    (key: (typeof PAD)[number]) => {
      if (busy || locked) return;
      const current = pinRef.current;
      if (key === "back") setPin(current.slice(0, -1));
      else if (key === "ok") {
        if (current.length >= 4) onSubmit(current);
      } else if (current.length < 8) setPin(current + key);
    },
    [busy, locked, onSubmit, setPin],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      if (/^\d$/.test(e.key)) press(e.key as (typeof PAD)[number]);
      else if (e.key === "Backspace") press("back");
      else if (e.key === "Enter") press("ok");
      else if (e.key === "Escape") onCancel();
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    // Heard first, on the way down, so nothing else on the page takes them too.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [press, onCancel]);

  return (
    <div className="flex flex-col items-center">
      {target ? (
        <>
          <Avatar user={target} size="lg" />
          <div className="mt-3 text-lg font-semibold">{target.name}</div>
        </>
      ) : null}
      <div className={cn("flex items-center gap-1.5 text-sm text-muted", target && "mt-1")}>
        <Lock className="size-3.5" /> {prompt}
      </div>
      <div className={cn("flex h-4 items-center gap-3", small ? "mt-4" : "mt-5")} aria-label={`${pin.length} digits entered`}>
        {Array.from({ length: Math.max(4, pin.length) }, (_, i) => (
          <span
            key={i}
            className={cn("size-3 rounded-full border transition-colors", i < pin.length ? "border-accent bg-accent" : "border-border-strong")}
          />
        ))}
      </div>
      <p role="alert" className={cn("min-h-5 text-center text-sm text-danger", small ? "mt-2" : "mt-3")}>
        {error}
      </p>
      <div className={cn("grid grid-cols-3", small ? "mt-1 gap-2.5" : "mt-2 gap-3")}>
        {PAD.map((key) => (
          <button
            key={key}
            type="button"
            disabled={busy || locked || (key === "ok" && pin.length < 4)}
            onClick={() => press(key)}
            aria-label={key === "back" ? "Delete digit" : key === "ok" ? submitLabel : key}
            className={cn(
              "flex items-center justify-center rounded-full font-semibold transition-colors disabled:opacity-40",
              small ? "size-12 text-lg" : "size-16 text-xl",
              key === "ok" ? "bg-accent text-accent-fg hover:bg-accent-hover" : "bg-surface-2 text-fg hover:bg-surface-3",
            )}
          >
            {key === "back" ? <Delete className="size-5" /> : key === "ok" ? busy ? <Spinner className="size-5 text-accent-fg" /> : "OK" : key}
          </button>
        ))}
      </div>
      <Button variant="ghost" className={small ? "mt-3" : "mt-6"} icon={cancelIcon} onClick={onCancel}>
        {cancelLabel}
      </Button>
    </div>
  );
}
