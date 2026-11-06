import { useState, type FormEvent } from "react";
import { Navigate, useNavigate, useSearchParams } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { errorMessage, isApiError } from "@/api/client";
import { fetchProfiles, useLogin, useMe, useServer } from "@/api/hooks";
import type { TwoStepSetup, User } from "@/api/types";
import { Splash } from "@/auth";
import { Button, Field, TextInput } from "@/components/ui";
import { useDocumentTitle } from "@/lib/hooks";
import { asksWhoIsWatching, lastProfile } from "@/lib/lastProfile";
import { appShell, SWITCH_SERVER_PATH } from "@/lib/shell";
import { codeKind, isTwoStepError, twoStepSetupOf } from "@/lib/twoStep";
import { AuthLayout } from "./AuthLayout";
import { safeNext } from "./ProfilesPage";
import { RecoveryCodesView, ScanSetup } from "./TwoStepSetup";

/**
 * After the password: a code from the authenticator app; or, when the server
 * requires two-step sign-in and the account doesn't use it yet, setting it up
 * (the secret came with the answer), then its recovery codes.
 */
type Step =
  | { kind: "password" }
  | { kind: "code" }
  | { kind: "setup"; setup: TwoStepSetup }
  | { kind: "codes"; codes: string[]; user: User };

export function LoginPage() {
  useDocumentTitle("Sign in");
  const [params] = useSearchParams();
  const next = safeNext(params.get("next"));
  /** Signing in to allow an app (the OAuth authorize page). */
  const forApp = next === "/oauth/authorize" || next.startsWith("/oauth/authorize?");
  /** Signing in to allow a TV or a Jellyfin app (the page for the code it shows). */
  const forTv = /^\/link([?#]|$)/.test(next);
  const navigate = useNavigate();
  const qc = useQueryClient();
  const server = useServer();
  const me = useMe(!server.data?.setup_required);
  const login = useLogin();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [signingIn, setSigningIn] = useState(false);
  const [step, setStep] = useState<Step>({ kind: "password" });
  const [code, setCode] = useState("");
  /** A recovery code instead of one from the app. */
  const [recovery, setRecovery] = useState(false);

  if (server.data?.setup_required) return <Navigate to="/setup" replace />;
  if (me.data && !signingIn) return <Navigate to={next} replace />;
  if (server.isPending || (me.isPending && me.fetchStatus === "fetching")) return <Splash />;

  // A household with several profiles picks one first, unless this browser
  // was last used as the profile signed in to and it has no PIN.
  const goOn = (user: User) => {
    void fetchProfiles(qc)
      .catch(() => [])
      .then((profiles) =>
        navigate(asksWhoIsWatching(user, profiles.length, lastProfile()) ? `/profiles?next=${encodeURIComponent(next)}` : next, {
          replace: true,
        }),
      );
  };

  const signIn = (withCode?: string, setupId?: string) => {
    setSigningIn(true);
    login.mutate(
      { username: username.trim(), password, ...(withCode ? { code: withCode } : {}), ...(setupId ? { setup_id: setupId } : {}) },
      {
        onSuccess: (res) => {
          // Set up on the way in: its recovery codes are shown once, first.
          if (res.recovery_codes?.length) setStep({ kind: "codes", codes: res.recovery_codes, user: res.user });
          else goOn(res.user);
        },
        onError: (err) => {
          setSigningIn(false);
          const setup = twoStepSetupOf(err);
          if (setup) setStep({ kind: "setup", setup });
          else if (!withCode && isTwoStepError(err)) setStep({ kind: "code" });
        },
      },
    );
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    signIn();
  };

  const onSubmitCode = (e: FormEvent) => {
    e.preventDefault();
    if (codeKind(code) && !login.isPending) signIn(code.trim());
  };

  const typeCode = (value: string) => {
    if (recovery) return setCode(value);
    const digits = value.replace(/\D/g, "").slice(0, 6);
    setCode(digits);
    // A whole code from the app goes right away.
    if (digits.length === 6 && !login.isPending) signIn(digits);
  };

  const chooseRecovery = (on: boolean) => {
    setRecovery(on);
    setCode("");
    login.reset();
  };

  const back = () => {
    setStep({ kind: "password" });
    setPassword("");
    chooseRecovery(false);
  };

  // 403: disabled account, 429: too many attempts; the server explains both,
  // and a wrong code. Asking for the code, or to set it up, isn't an error.
  const error = !login.error
    ? null
    : isTwoStepError(login.error) || twoStepSetupOf(login.error)
      ? login.variables?.code
        ? errorMessage(login.error)
        : null
      : isApiError(login.error, 401)
        ? "Wrong username or password."
        : errorMessage(login.error);

  if (step.kind === "codes") {
    return (
      <AuthLayout title="Save your recovery codes" subtitle="Two-step sign-in is on.">
        <RecoveryCodesView
          codes={step.codes}
          username={step.user.username}
          serverName={server.data?.name}
          onDone={() => goOn(step.user)}
        />
      </AuthLayout>
    );
  }

  if (step.kind === "setup") {
    return (
      <AuthLayout
        title="Set up two-step sign-in"
        subtitle={`${server.data?.name ?? "This server"} asks everyone for a code from an authenticator app after their password. It takes a minute.`}
      >
        <ScanSetup
          // A new secret (the last one expired) starts the code over.
          key={step.setup.secret}
          stacked
          appsHint
          setup={step.setup}
          busy={login.isPending || (signingIn && !login.isError)}
          error={error}
          submitLabel="Turn on and sign in"
          onCode={(value) => signIn(value, step.setup.id)}
          onCancel={back}
          cancelLabel="Back"
        />
      </AuthLayout>
    );
  }

  if (step.kind === "code") {
    return (
      <AuthLayout
        title="Two-step sign-in"
        subtitle={
          recovery
            ? "Enter one of the recovery codes you saved when you turned it on. Each works once."
            : "Enter the 6-digit code from your authenticator app."
        }
      >
        <form onSubmit={onSubmitCode} className="space-y-4">
          <Field label={recovery ? "Recovery code" : "Code"} htmlFor="code">
            <TextInput
              key={recovery ? "recovery" : "app"}
              id="code"
              autoFocus
              autoComplete={recovery ? "off" : "one-time-code"}
              inputMode={recovery ? "text" : "numeric"}
              autoCapitalize="off"
              spellCheck={false}
              // An app's code keeps its 6 digits whatever comes with them ("123 456" pasted).
              maxLength={recovery ? 24 : undefined}
              placeholder={recovery ? "xxxx-xxxx-xxxx-xxxx" : undefined}
              value={code}
              onChange={(e) => typeCode(e.target.value)}
              className={
                recovery ? "font-mono" : "indent-[0.4em] text-center text-2xl font-semibold tracking-[0.4em] tabular-nums"
              }
              required
            />
          </Field>
          {error ? (
            <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
              {error}
            </p>
          ) : null}
          <Button
            type="submit"
            variant="primary"
            size="lg"
            className="w-full"
            loading={login.isPending || (signingIn && !login.isError)}
            disabled={!codeKind(code)}
          >
            Sign in
          </Button>
          <div className="flex items-center justify-between gap-3 text-sm">
            <button type="button" className="font-medium text-accent hover:text-accent-hover" onClick={() => chooseRecovery(!recovery)}>
              {recovery ? "Use your authenticator app" : "Use a recovery code"}
            </button>
            <button type="button" className="text-muted hover:text-fg" onClick={back}>
              Back
            </button>
          </div>
        </form>
      </AuthLayout>
    );
  }

  return (
    <AuthLayout
      title="Sign in"
      subtitle={
        server.data
          ? forApp
            ? `to let an app use ${server.data.name}`
            : forTv
              ? `to let your TV use ${server.data.name}`
              : `to ${server.data.name}`
          : undefined
      }
    >
      <form onSubmit={onSubmit} className="space-y-4">
        <Field label="Username" htmlFor="username">
          <TextInput
            id="username"
            autoComplete="username"
            autoFocus
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            required
          />
        </Field>
        <Field label="Password" htmlFor="password">
          <TextInput
            id="password"
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
          />
        </Field>
        {error ? (
          <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
            {error}
          </p>
        ) : null}
        <Button type="submit" variant="primary" size="lg" className="w-full" loading={login.isPending || (signingIn && !login.isError)}>
          Sign in
        </Button>
        {appShell() ? (
          <p className="text-center text-xs text-subtle">
            <a href={SWITCH_SERVER_PATH} className="font-medium text-accent hover:text-accent-hover">
              Use another server
            </a>
          </p>
        ) : null}
      </form>
    </AuthLayout>
  );
}
