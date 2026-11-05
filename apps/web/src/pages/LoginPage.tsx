import { useState, type FormEvent } from "react";
import { Navigate, useNavigate, useSearchParams } from "react-router";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { errorMessage, isApiError } from "@/api/client";
import { fetchProfiles, useLogin, useMe, useServer } from "@/api/hooks";
import type { TwoStepSetup } from "@/api/types";
import { Splash } from "@/auth";
import { Button, Field, TextInput } from "@/components/ui";
import { useDocumentTitle } from "@/lib/hooks";
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
  | { kind: "codes"; codes: string[]; username: string };

export function LoginPage() {
  const { t } = useTranslation(["pages", "common"]);
  useDocumentTitle(t("login.signIn"));
  const [params] = useSearchParams();
  const next = safeNext(params.get("next"));
  /** Signing in to allow an app (the OAuth authorize page). */
  const forApp = next === "/oauth/authorize" || next.startsWith("/oauth/authorize?");
  /** Signing in to allow a TV (the page for the code it shows). */
  const forTv = next === "/link" || next.startsWith("/link?");
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

  // A household with several profiles picks one first.
  const goOn = () => {
    void fetchProfiles(qc)
      .catch(() => [])
      .then((profiles) => navigate(profiles.length > 1 ? `/profiles?next=${encodeURIComponent(next)}` : next, { replace: true }));
  };

  const signIn = (withCode?: string, setupId?: string) => {
    setSigningIn(true);
    login.mutate(
      { username: username.trim(), password, ...(withCode ? { code: withCode } : {}), ...(setupId ? { setup_id: setupId } : {}) },
      {
        onSuccess: (res) => {
          // Set up on the way in: its recovery codes are shown once, first.
          if (res.recovery_codes?.length) setStep({ kind: "codes", codes: res.recovery_codes, username: res.user.username });
          else goOn();
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
        ? t("login.wrongPassword")
        : errorMessage(login.error);

  if (step.kind === "codes") {
    return (
      <AuthLayout title={t("twoStepSetup.saveYourCodes")} subtitle={t("login.codesSubtitle")}>
        <RecoveryCodesView codes={step.codes} username={step.username} serverName={server.data?.name} onDone={goOn} />
      </AuthLayout>
    );
  }

  if (step.kind === "setup") {
    return (
      <AuthLayout
        title={t("twoStepSetup.setUpTitle")}
        subtitle={t("login.setupSubtitle", { server: server.data?.name ?? t("twoStepSetup.thisServer") })}
      >
        <ScanSetup
          // A new secret (the last one expired) starts the code over.
          key={step.setup.secret}
          stacked
          appsHint
          setup={step.setup}
          busy={login.isPending || (signingIn && !login.isError)}
          error={error}
          submitLabel={t("twoStepSetup.turnOnAndSignIn")}
          onCode={(value) => signIn(value, step.setup.id)}
          onCancel={back}
          cancelLabel={t("common:actions.back")}
        />
      </AuthLayout>
    );
  }

  if (step.kind === "code") {
    return (
      <AuthLayout title={t("login.codeTitle")} subtitle={recovery ? t("login.recoveryHint") : t("login.codeHint")}>
        <form onSubmit={onSubmitCode} className="space-y-4">
          <Field label={recovery ? t("login.recoveryCode") : t("fields.code")} htmlFor="code">
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
            {t("login.signIn")}
          </Button>
          <div className="flex items-center justify-between gap-3 text-sm">
            <button type="button" className="font-medium text-accent hover:text-accent-hover" onClick={() => chooseRecovery(!recovery)}>
              {recovery ? t("login.useApp") : t("login.useRecoveryCode")}
            </button>
            <button type="button" className="text-muted hover:text-fg" onClick={back}>
              {t("common:actions.back")}
            </button>
          </div>
        </form>
      </AuthLayout>
    );
  }

  return (
    <AuthLayout
      title={t("login.signIn")}
      subtitle={
        server.data
          ? t(forApp ? "login.toServerForApp" : forTv ? "login.toServerForTv" : "login.toServer", { server: server.data.name })
          : undefined
      }
    >
      <form onSubmit={onSubmit} className="space-y-4">
        <Field label={t("fields.username")} htmlFor="username">
          <TextInput
            id="username"
            autoComplete="username"
            autoFocus
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            required
          />
        </Field>
        <Field label={t("fields.password")} htmlFor="password">
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
          {t("login.signIn")}
        </Button>
        {appShell() ? (
          <p className="text-center text-xs text-subtle">
            <a href={SWITCH_SERVER_PATH} className="font-medium text-accent hover:text-accent-hover">
              {t("login.otherServer")}
            </a>
          </p>
        ) : null}
      </form>
    </AuthLayout>
  );
}
