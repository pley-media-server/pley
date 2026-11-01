import { useState, type FormEvent, type ReactNode } from "react";
import { Copy, Download, Smartphone } from "lucide-react";
import type { TwoStepSetup } from "@/api/types";
import { QrCode } from "@/components/QrCode";
import { useToast } from "@/components/Toast";
import { Button, buttonClass, Field, IconButton, TextInput } from "@/components/ui";
import { copyText } from "@/lib/clipboard";
import { cn } from "@/lib/cn";
import { appShell } from "@/lib/shell";
import { downloadText, groupSecret, recoveryCodesText } from "@/lib/twoStep";

/** A phone or tablet, whose own screen its camera can't scan. */
function isTouchDevice(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.("(pointer: coarse)").matches;
}

/**
 * Adding Pley to an authenticator app, then its first code. On a phone, a
 * link opens the app on it (its camera can't scan its own screen), with the
 * QR code for another device; elsewhere the QR code. The key, to type in,
 * for either. A whole code goes as soon as it's typed or pasted.
 */
export function ScanSetup({
  setup,
  busy,
  error,
  submitLabel,
  onCode,
  onCancel,
  cancelLabel = "Cancel",
  stacked = false,
  appsHint = false,
}: {
  setup: TwoStepSetup;
  busy: boolean;
  error: string | null;
  submitLabel: string;
  onCode: (code: string) => void;
  onCancel?: () => void;
  cancelLabel?: string;
  /** One column, for a narrow card: the QR code above the steps. */
  stacked?: boolean;
  /** Name a few apps, for someone who wasn't asked whether they have one. */
  appsHint?: boolean;
}) {
  const toast = useToast();
  const [code, setCode] = useState("");
  const touch = isTouchDevice();

  const type = (value: string) => {
    const digits = value.replace(/\D/g, "").slice(0, 6);
    setCode(digits);
    if (digits.length === 6 && !busy) onCode(digits);
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (code.length === 6 && !busy) onCode(code);
  };

  const qr = (
    <QrCode
      size={setup.qr.size}
      path={setup.qr.path}
      label="QR code that adds Pley to an authenticator app"
      className={cn("mx-auto shrink-0", touch ? "size-36" : "size-44", !stacked && "sm:mx-0")}
    />
  );

  return (
    <div className="space-y-4">
      <div className={cn("grid gap-5", !stacked && "sm:grid-cols-[auto_1fr] sm:items-start")}>
        {touch ? null : qr}
        <div className="min-w-0 space-y-3 text-sm">
          <ol className="list-decimal space-y-1.5 pl-5 text-muted">
            <li>{touch ? "Add Pley to an authenticator app on this phone." : "Scan this QR code with your authenticator app."}</li>
            <li>Enter the 6-digit code the app shows for Pley.</li>
          </ol>
          {touch ? (
            <a href={setup.uri} className={buttonClass("outline", "md", "w-full")}>
              <Smartphone className="size-4" aria-hidden />
              Open in an authenticator app
            </a>
          ) : null}
          {appsHint ? (
            <p className="text-xs text-subtle">
              Google Authenticator, Microsoft Authenticator, 1Password, Bitwarden and most password managers work.
            </p>
          ) : null}
          <div className="rounded-md border border-border bg-surface-2 px-3 py-2">
            <div className="text-xs text-subtle">
              {touch ? "No app on this phone? Scan the QR code below with another device, or enter this key:" : "Can't scan it? Enter this key in the app instead:"}
            </div>
            <div className="mt-1 flex items-center gap-1">
              {/* Wraps between its groups of four, as they're typed. */}
              <code className="min-w-0 flex-1 break-words font-mono text-sm text-fg">{groupSecret(setup.secret)}</code>
              <IconButton
                label="Copy the key"
                size="sm"
                onClick={() =>
                  void copyText(setup.secret).then((ok) => (ok ? toast.success("Key copied") : toast.error("Couldn't copy the key")))
                }
              >
                <Copy className="size-4" />
              </IconButton>
            </div>
          </div>
          {touch ? qr : null}
        </div>
      </div>
      <form onSubmit={submit} className="space-y-3">
        <Field label="Code from the app" htmlFor="two-step-code">
          <TextInput
            id="two-step-code"
            inputMode="numeric"
            autoComplete="one-time-code"
            // Not on a phone, whose keyboard would cover the link to the app.
            autoFocus={!touch}
            // No maxLength: a code pasted as "123 456" keeps its 6 digits.
            value={code}
            onChange={(e) => type(e.target.value)}
            className="max-w-40 tracking-[0.3em] tabular-nums"
          />
        </Field>
        {error ? (
          <p role="alert" className="text-sm text-danger">
            {error}
          </p>
        ) : null}
        <div className="flex justify-end gap-2">
          {onCancel ? (
            <Button variant="ghost" onClick={onCancel} disabled={busy}>
              {cancelLabel}
            </Button>
          ) : null}
          <Button type="submit" variant="primary" loading={busy} disabled={code.length !== 6}>
            {submitLabel}
          </Button>
        </div>
      </form>
    </div>
  );
}

/** Recovery codes, shown once: to copy or save, and to say they were saved. */
export function RecoveryCodesView({
  codes,
  username,
  serverName,
  note,
  doneLabel = "I've saved them",
  onDone,
}: {
  codes: string[];
  username: string;
  serverName?: string;
  /** A line under the codes. */
  note?: ReactNode;
  doneLabel?: string;
  onDone: () => void;
}) {
  const toast = useToast();
  const text = recoveryCodesText(codes, username, serverName);

  return (
    <div className="space-y-3">
      <p className="text-sm text-muted">
        Save these recovery codes somewhere safe, like a password manager. If you lose your phone, each one signs in once
        instead of a code from the app. They won't be shown again.
      </p>
      {/* Two whole codes to a row where they fit, else one: a code split over two lines gets copied wrong. */}
      <ul
        aria-label="Recovery codes"
        className="grid grid-cols-[repeat(auto-fill,minmax(max(20ch,40%),1fr))] gap-x-8 gap-y-1 rounded-md border border-border bg-surface-2 px-4 py-3 font-mono text-sm text-fg"
      >
        {codes.map((c) => (
          <li key={c} className="whitespace-nowrap">
            {c}
          </li>
        ))}
      </ul>
      {note ? <p className="text-xs text-subtle">{note}</p> : null}
      <div className="flex flex-wrap justify-end gap-2">
        <Button
          variant="ghost"
          icon={<Copy className="size-4" />}
          onClick={() => void copyText(text).then((ok) => (ok ? toast.success("Recovery codes copied") : toast.error("Couldn't copy them")))}
        >
          Copy
        </Button>
        {/* The apps' web views don't save files. */}
        {appShell() ? null : (
          <Button variant="ghost" icon={<Download className="size-4" />} onClick={() => downloadText("pley-recovery-codes.txt", text)}>
            Download
          </Button>
        )}
        <Button variant="primary" onClick={onDone}>
          {doneLabel}
        </Button>
      </div>
    </div>
  );
}
