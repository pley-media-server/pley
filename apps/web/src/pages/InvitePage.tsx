import { useState, type FormEvent } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useTranslation } from "react-i18next";
import { TicketX } from "lucide-react";
import { errorMessage, isApiError } from "@/api/client";
import { useAcceptInvite, useInviteCheck, useLogin, useLogout, useMe } from "@/api/hooks";
import type { TwoStepSetup } from "@/api/types";
import { Splash } from "@/auth";
import { PasswordInput } from "@/components/PasswordInput";
import { Button, buttonClass, Field, TextInput } from "@/components/ui";
import { formatRelative } from "@/lib/format";
import { useDocumentTitle } from "@/lib/hooks";
import { MIN_PASSWORD_LENGTH, passwordProblem } from "@/lib/passwords";
import { twoStepSetupOf } from "@/lib/twoStep";
import { AuthLayout } from "./AuthLayout";
import { RecoveryCodesView, ScanSetup } from "./TwoStepSetup";

/**
 * Joining a server that requires two-step sign-in: the account is made, and
 * signing in to it sets it up (the secret came with the answer), then its
 * recovery codes are shown.
 */
type Joining = { kind: "setup"; setup: TwoStepSetup } | { kind: "codes"; codes: string[]; username: string };

/** `/invite/:code`: create an account with an invitation link. */
export function InvitePage() {
  const { t } = useTranslation(["pages", "common"]);
  useDocumentTitle(t("invite.documentTitle"));
  const { code = "" } = useParams();
  const navigate = useNavigate();
  const check = useInviteCheck(code);
  const me = useMe();
  const logout = useLogout();
  const accept = useAcceptInvite(code);
  const login = useLogin();
  const [username, setUsername] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [password, setPassword] = useState("");
  const [localError, setLocalError] = useState<string | null>(null);
  const [joining, setJoining] = useState<Joining | null>(null);

  if (joining?.kind === "codes") {
    return (
      <AuthLayout title={t("twoStepSetup.saveYourCodes")} subtitle={t("invite.codesSubtitle")}>
        <RecoveryCodesView
          codes={joining.codes}
          username={joining.username}
          serverName={check.data?.server_name}
          onDone={() => void navigate("/", { replace: true })}
        />
      </AuthLayout>
    );
  }

  if (joining?.kind === "setup") {
    const setUp = (value: string) =>
      login.mutate(
        { username: username.trim(), password, code: value, setup_id: joining.setup.id },
        {
          onSuccess: (res) =>
            res.recovery_codes?.length
              ? setJoining({ kind: "codes", codes: res.recovery_codes, username: res.user.username })
              : void navigate("/", { replace: true }),
          onError: (err) => {
            const setup = twoStepSetupOf(err);
            if (setup) setJoining({ kind: "setup", setup });
          },
        },
      );
    return (
      <AuthLayout
        title={t("twoStepSetup.setUpTitle")}
        subtitle={t("invite.setupSubtitle", { server: check.data?.server_name ?? t("twoStepSetup.thisServer") })}
      >
        <ScanSetup
          key={joining.setup.secret}
          stacked
          appsHint
          setup={joining.setup}
          busy={login.isPending}
          error={login.error ? errorMessage(login.error) : null}
          submitLabel={t("twoStepSetup.turnOnAndSignIn")}
          onCode={setUp}
        />
      </AuthLayout>
    );
  }

  if (check.isPending) return <Splash />;
  if (check.isError) {
    const gone = isApiError(check.error, 404);
    return (
      <AuthLayout title={gone ? t("invite.cantBeUsed") : t("common:states.somethingWentWrong")}>
        <div className="space-y-5">
          <div className="flex items-start gap-3 text-sm text-muted">
            <TicketX className="mt-0.5 size-5 shrink-0 text-subtle" />
            <p>{gone ? t("invite.cantBeUsedHint") : errorMessage(check.error)}</p>
          </div>
          <Link to="/login" className={buttonClass("primary", "md", "w-full")}>
            {t("invite.goToSignIn")}
          </Link>
        </div>
      </AuthLayout>
    );
  }

  const server = check.data.server_name;

  if (me.data && !accept.isSuccess) {
    return (
      <AuthLayout title={t("invite.join", { server })} subtitle={t("invite.signedInAs", { name: me.data.name })}>
        <div className="space-y-3">
          <p className="text-sm text-muted">{t("invite.signOutFirst")}</p>
          <Button
            variant="primary"
            className="w-full"
            loading={logout.isPending}
            onClick={() => logout.mutate(undefined, { onSettled: () => void me.refetch() })}
          >
            {t("invite.signOutAndContinue")}
          </Button>
          <Button variant="ghost" className="w-full" onClick={() => void navigate("/", { replace: true })}>
            {t("invite.staySignedIn")}
          </Button>
        </div>
      </AuthLayout>
    );
  }

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    setLocalError(null);
    if (!username.trim()) return setLocalError(t("invite.chooseUsername"));
    const problem = passwordProblem(password, username);
    if (problem) return setLocalError(problem);
    accept.mutate(
      { username: username.trim(), password, display_name: displayName.trim() || undefined },
      {
        onSuccess: () => void navigate("/", { replace: true }),
        onError: (err) => {
          const setup = twoStepSetupOf(err);
          if (setup) setJoining({ kind: "setup", setup });
        },
      },
    );
  };

  const error = localError ?? (accept.error && !twoStepSetupOf(accept.error) ? errorMessage(accept.error) : null);

  return (
    <AuthLayout
      title={t("invite.invited", { server })}
      subtitle={
        check.data.expires_at
          ? t("invite.createToWatchExpires", { when: formatRelative(check.data.expires_at) })
          : t("invite.createToWatch")
      }
    >
      <form onSubmit={onSubmit} className="space-y-4">
        <Field label={t("fields.username")} htmlFor="invite-username" hint={t("invite.usernameHint")}>
          <TextInput
            id="invite-username"
            autoComplete="username"
            autoFocus
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            required
          />
        </Field>
        <Field label={t("invite.yourName")} htmlFor="invite-name" hint={t("invite.yourNameHint")}>
          <TextInput id="invite-name" autoComplete="name" value={displayName} onChange={(e) => setDisplayName(e.target.value)} />
        </Field>
        <Field label={t("fields.password")} htmlFor="invite-password" hint={t("invite.passwordHint", { min: MIN_PASSWORD_LENGTH })}>
          <PasswordInput id="invite-password" autoComplete="new-password" value={password} onChange={(e) => setPassword(e.target.value)} required />
        </Field>
        {error ? (
          <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
            {error}
          </p>
        ) : null}
        <Button type="submit" variant="primary" size="lg" className="w-full" loading={accept.isPending}>
          {t("setup.createAccount")}
        </Button>
        <p className="text-center text-xs text-subtle">
          {t("invite.haveAccount")}{" "}
          <Link to="/login" className="font-medium text-accent hover:text-accent-hover">
            {t("login.signIn")}
          </Link>
        </p>
      </form>
    </AuthLayout>
  );
}
