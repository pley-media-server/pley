import { useState, type FormEvent } from "react";
import { Link, useNavigate, useParams } from "react-router";
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
  useDocumentTitle("Invitation");
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
      <AuthLayout title="Save your recovery codes" subtitle="Your account is ready, and two-step sign-in is on.">
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
        title="Set up two-step sign-in"
        subtitle={`Your account is made. ${check.data?.server_name ?? "This server"} asks everyone for a code from an authenticator app after their password.`}
      >
        <ScanSetup
          key={joining.setup.secret}
          stacked
          appsHint
          setup={joining.setup}
          busy={login.isPending}
          error={login.error ? errorMessage(login.error) : null}
          submitLabel="Turn on and sign in"
          onCode={setUp}
        />
      </AuthLayout>
    );
  }

  if (check.isPending) return <Splash />;
  if (check.isError) {
    const gone = isApiError(check.error, 404);
    return (
      <AuthLayout title={gone ? "This invitation can't be used" : "Something went wrong"}>
        <div className="space-y-5">
          <div className="flex items-start gap-3 text-sm text-muted">
            <TicketX className="mt-0.5 size-5 shrink-0 text-subtle" />
            <p>
              {gone
                ? "The link is wrong, has expired, or has already been used. Ask the person who sent it for a new one."
                : errorMessage(check.error)}
            </p>
          </div>
          <Link to="/login" className={buttonClass("primary", "md", "w-full")}>
            Go to sign in
          </Link>
        </div>
      </AuthLayout>
    );
  }

  const server = check.data.server_name;

  if (me.data && !accept.isSuccess) {
    return (
      <AuthLayout title={`Join ${server}`} subtitle={`You're signed in as ${me.data.name}.`}>
        <div className="space-y-3">
          <p className="text-sm text-muted">To create a new account with this invitation, sign out first.</p>
          <Button
            variant="primary"
            className="w-full"
            loading={logout.isPending}
            onClick={() => logout.mutate(undefined, { onSettled: () => void me.refetch() })}
          >
            Sign out and continue
          </Button>
          <Button variant="ghost" className="w-full" onClick={() => void navigate("/", { replace: true })}>
            Stay signed in
          </Button>
        </div>
      </AuthLayout>
    );
  }

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    setLocalError(null);
    if (!username.trim()) return setLocalError("Choose a username.");
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
      title={`You're invited to ${server}`}
      subtitle={
        check.data.expires_at
          ? `Create your account to start watching. This invitation expires ${formatRelative(check.data.expires_at)}.`
          : "Create your account to start watching."
      }
    >
      <form onSubmit={onSubmit} className="space-y-4">
        <Field label="Username" htmlFor="invite-username" hint="Letters, digits and _ - . @">
          <TextInput
            id="invite-username"
            autoComplete="username"
            autoFocus
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            required
          />
        </Field>
        <Field label="Your name" htmlFor="invite-name" hint="Optional: how Pley greets you.">
          <TextInput id="invite-name" autoComplete="name" value={displayName} onChange={(e) => setDisplayName(e.target.value)} />
        </Field>
        <Field label="Password" htmlFor="invite-password" hint={`At least ${MIN_PASSWORD_LENGTH} characters.`}>
          <PasswordInput id="invite-password" autoComplete="new-password" value={password} onChange={(e) => setPassword(e.target.value)} required />
        </Field>
        {error ? (
          <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
            {error}
          </p>
        ) : null}
        <Button type="submit" variant="primary" size="lg" className="w-full" loading={accept.isPending}>
          Create account
        </Button>
        <p className="text-center text-xs text-subtle">
          Already have an account?{" "}
          <Link to="/login" className="font-medium text-accent hover:text-accent-hover">
            Sign in
          </Link>
        </p>
      </form>
    </AuthLayout>
  );
}
