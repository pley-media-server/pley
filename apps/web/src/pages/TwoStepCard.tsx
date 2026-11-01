import { useState, type FormEvent, type ReactNode } from "react";
import { ShieldCheck } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useDisableTwoStep, useEnableTwoStep, useNewRecoveryCodes, useServer, useStartTwoStep, useTwoStep } from "@/api/hooks";
import type { TwoStepSetup, TwoStepStatus } from "@/api/types";
import { useAuth } from "@/auth";
import { useToast } from "@/components/Toast";
import { Button, Card, CardTitle, Field, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";
import { formatDay, pluralize } from "@/lib/format";
import { codeKind } from "@/lib/twoStep";
import { RecoveryCodesView, ScanSetup } from "./TwoStepSetup";

/**
 * Where the card is: showing the status, asking for the password (and, to
 * move to another app, a code of the current one), scanning, turning off,
 * or showing new codes.
 */
type Stage =
  | { kind: "status" }
  | { kind: "password"; next: "setup" | "codes" | "move" }
  | { kind: "scan"; setup: TwoStepSetup; moving: boolean }
  | { kind: "off" }
  | { kind: "codes"; codes: string[]; after: "on" | "moved" | "renewed" };

/** Recovery codes left at which the card suggests making new ones. */
const FEW_CODES = 3;

/** Two-step sign-in: a code from an authenticator app after the password. */
export function TwoStepCard() {
  const user = useAuth();
  const usable = !user.managed && user.has_password;
  const status = useTwoStep(usable);
  const [stage, setStage] = useState<Stage>({ kind: "status" });
  if (!usable) return null;
  const done = () => setStage({ kind: "status" });

  return (
    <Card>
      <CardTitle icon={<ShieldCheck />}>Two-step sign-in</CardTitle>
      {stage.kind === "password" ? (
        <PasswordStep
          next={stage.next}
          onCancel={done}
          onSetup={(setup) => setStage({ kind: "scan", setup, moving: stage.next === "move" })}
          onCodes={(codes) => setStage({ kind: "codes", codes, after: "renewed" })}
        />
      ) : stage.kind === "scan" ? (
        <ScanStep
          setup={stage.setup}
          moving={stage.moving}
          onCancel={done}
          onDone={(codes) => setStage({ kind: "codes", codes, after: stage.moving ? "moved" : "on" })}
        />
      ) : stage.kind === "off" ? (
        <TurnOff onCancel={done} onOff={done} />
      ) : stage.kind === "codes" ? (
        <SavedCodes codes={stage.codes} after={stage.after} onDone={done} />
      ) : status.isPending ? (
        <p className="text-sm text-subtle">Loading…</p>
      ) : status.isError ? (
        <p className="text-sm text-danger">{errorMessage(status.error)}</p>
      ) : (
        <Status
          status={status.data}
          onSetUp={() => setStage({ kind: "password", next: "setup" })}
          onMove={() => setStage({ kind: "password", next: "move" })}
          onNewCodes={() => setStage({ kind: "password", next: "codes" })}
          onTurnOff={() => setStage({ kind: "off" })}
        />
      )}
    </Card>
  );
}

function Status({
  status,
  onSetUp,
  onMove,
  onNewCodes,
  onTurnOff,
}: {
  status: TwoStepStatus;
  onSetUp: () => void;
  onMove: () => void;
  onNewCodes: () => void;
  onTurnOff: () => void;
}) {
  // The server requires it: it stays on.
  const required = !!useServer().data?.two_step_required;
  if (!status.enabled) {
    return (
      <>
        <p className="text-sm text-muted">
          After your password, Pley asks for a code from an authenticator app on your phone, so a password alone can't
          sign in to your account.
        </p>
        <div className="mt-4 flex justify-end">
          <Button variant="primary" onClick={onSetUp}>
            Set up
          </Button>
        </div>
      </>
    );
  }
  const left = status.recovery_codes_left;
  return (
    <>
      <p className="text-sm text-muted">
        On since {formatDay(status.since)}: signing in takes your password and a code from your authenticator app.
        {required ? " This server requires it, so it stays on." : null}
      </p>
      <p className={cn("mt-2 text-sm", left <= FEW_CODES ? "text-danger" : "text-subtle")}>
        {left === 0
          ? "No recovery codes left: make new ones, so a lost phone doesn't lock you out."
          : left <= FEW_CODES
            ? `Only ${pluralize(left, "recovery code")} left: make new ones before they run out.`
            : `${pluralize(left, "recovery code")} left, for when your phone isn't at hand.`}
      </p>
      <div className="mt-4 flex flex-wrap justify-end gap-2">
        {required ? null : (
          <Button variant="ghost" onClick={onTurnOff}>
            Turn off
          </Button>
        )}
        <Button variant="ghost" onClick={onMove} title="For a new phone, or another authenticator app">
          Use another app
        </Button>
        <Button variant="outline" onClick={onNewCodes}>
          New recovery codes
        </Button>
      </div>
    </>
  );
}

/** New recovery codes, shown once, with what came before them. */
function SavedCodes({ codes, after, onDone }: { codes: string[]; after: "on" | "moved" | "renewed"; onDone: () => void }) {
  const user = useAuth();
  const server = useServer();
  return (
    <RecoveryCodesView
      codes={codes}
      username={user.username}
      serverName={server.data?.name}
      note={
        after === "on"
          ? "Devices already signed in stay signed in. Sign out any you don't know under Devices."
          : after === "moved"
            ? "Codes from your old authenticator app, and your old recovery codes, don't work any more."
            : undefined
      }
      onDone={onDone}
    />
  );
}

function Problem({ children }: { children: ReactNode }) {
  return children ? <p className="text-sm text-danger">{children}</p> : null;
}

/**
 * Changing how the account signs in takes its password: a session alone
 * isn't enough. Moving to another app takes a code of the current one too
 * (or a recovery code, for a phone that's gone).
 */
function PasswordStep({
  next,
  onCancel,
  onSetup,
  onCodes,
}: {
  next: "setup" | "codes" | "move";
  onCancel: () => void;
  onSetup: (setup: TwoStepSetup) => void;
  onCodes: (codes: string[]) => void;
}) {
  const start = useStartTwoStep();
  const codes = useNewRecoveryCodes();
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const busy = start.isPending || codes.isPending;
  const moving = next === "move";

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    const onError = (err: unknown) => setError(errorMessage(err));
    if (next === "codes") codes.mutate(password, { onSuccess: (res) => onCodes(res.recovery_codes), onError });
    else start.mutate(moving ? { password, code: code.trim() } : { password }, { onSuccess: onSetup, onError });
  };

  return (
    <form onSubmit={onSubmit} className="space-y-3">
      <p className="text-sm text-muted">
        {next === "setup"
          ? "You'll need an authenticator app on your phone, such as Google Authenticator, Microsoft Authenticator, 1Password or Bitwarden."
          : moving
            ? "For a new phone or another authenticator app. Until you enter a code from the new one, the current one keeps working."
            : "New recovery codes replace the ones you have: those stop working."}
      </p>
      <div className={cn("grid gap-3", moving && "sm:grid-cols-2")}>
        <Field label="Your password" htmlFor="two-step-password">
          <TextInput
            id="two-step-password"
            type="password"
            autoComplete="current-password"
            autoFocus
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
          />
        </Field>
        {moving ? (
          <Field label="Code from your current app" htmlFor="two-step-current-code" hint="Or a recovery code.">
            <TextInput
              id="two-step-current-code"
              autoComplete="one-time-code"
              autoCapitalize="off"
              spellCheck={false}
              value={code}
              onChange={(e) => setCode(e.target.value)}
              required
            />
          </Field>
        ) : null}
      </div>
      <Problem>{error}</Problem>
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel} disabled={busy}>
          Cancel
        </Button>
        <Button type="submit" variant="primary" loading={busy} disabled={!password || (moving && !codeKind(code))}>
          {next === "codes" ? "Make new codes" : "Continue"}
        </Button>
      </div>
    </form>
  );
}

/** The QR code (or the key, or a link on phones) and the first code from the app, which turns it on or moves it. */
function ScanStep({
  setup,
  moving,
  onCancel,
  onDone,
}: {
  setup: TwoStepSetup;
  moving: boolean;
  onCancel: () => void;
  onDone: (codes: string[]) => void;
}) {
  const enable = useEnableTwoStep();
  const toast = useToast();
  const [error, setError] = useState<string | null>(null);

  const confirm = (code: string) => {
    setError(null);
    enable.mutate(code, {
      onSuccess: (res) => {
        toast.success(moving ? "Two-step sign-in uses your new app" : "Two-step sign-in is on");
        onDone(res.recovery_codes);
      },
      onError: (err) => setError(errorMessage(err)),
    });
  };

  return (
    <ScanSetup
      setup={setup}
      busy={enable.isPending}
      error={error}
      submitLabel={moving ? "Use this app" : "Turn on"}
      onCode={confirm}
      onCancel={onCancel}
    />
  );
}

/** Turning it off takes the password and a code: neither alone can. */
function TurnOff({ onCancel, onOff }: { onCancel: () => void; onOff: () => void }) {
  const disable = useDisableTwoStep();
  const toast = useToast();
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    disable.mutate(
      { password, code: code.trim() },
      {
        onSuccess: () => {
          toast.success("Two-step sign-in is off");
          onOff();
        },
        onError: (err) => setError(errorMessage(err)),
      },
    );
  };

  return (
    <form onSubmit={onSubmit} className="space-y-3">
      <p className="text-sm text-muted">Turning it off takes your password and a code from the app, or a recovery code.</p>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Your password" htmlFor="two-step-off-password">
          <TextInput
            id="two-step-off-password"
            type="password"
            autoComplete="current-password"
            autoFocus
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
          />
        </Field>
        <Field label="Code" htmlFor="two-step-off-code">
          <TextInput
            id="two-step-off-code"
            autoComplete="one-time-code"
            autoCapitalize="off"
            spellCheck={false}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            required
          />
        </Field>
      </div>
      <Problem>{error}</Problem>
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel} disabled={disable.isPending}>
          Cancel
        </Button>
        <Button type="submit" variant="danger" loading={disable.isPending} disabled={!password || !codeKind(code)}>
          Turn off
        </Button>
      </div>
    </form>
  );
}
