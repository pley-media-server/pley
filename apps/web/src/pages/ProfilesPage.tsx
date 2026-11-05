import { useState, type FormEvent } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { useTranslation } from "react-i18next";
import { Lock } from "lucide-react";
import { errorMessage, isApiError } from "@/api/client";
import { useProfiles, useServer, useSwitchProfile } from "@/api/hooks";
import type { ServerStatus, TwoStepSetup, User } from "@/api/types";
import { useOptionalAudioPlayer } from "@/audio/AudioPlayer";
import { useAuth } from "@/auth";
import { Avatar } from "@/components/Avatar";
import { Logo } from "@/components/Logo";
import { PinPad } from "@/components/PinPad";
import { Button, Spinner, TextInput } from "@/components/ui";
import { t as translate } from "@/i18n";
import { cn } from "@/lib/cn";
import { useDocumentTitle } from "@/lib/hooks";
import { codeKind, mustSetUpTwoStep, twoStepSetupOf } from "@/lib/twoStep";
import { RecoveryCodesView, ScanSetup } from "./TwoStepSetup";

/**
 * Where to go after signing in (`?next=`): a path on this site, but not the
 * sign-in or profile screens. URL parsing decides what is another site: it
 * reads "/\evil.com" and "/\t/evil.com" as "//evil.com".
 */
export function safeNext(next: string | null): string {
  if (!next?.startsWith("/")) return "/";
  let url: URL;
  try {
    url = new URL(next, window.location.origin);
  } catch {
    return "/";
  }
  if (url.origin !== window.location.origin) return "/";
  const path = `${url.pathname}${url.search}${url.hash}`;
  return path.startsWith("/login") || path.startsWith("/profiles") ? "/" : path;
}

/**
 * Whether `target` can see more than `me`: an administrator, a higher age
 * limit, or libraries `me` can't open. Switching up asks for a secret (the
 * server decides the same way).
 */
export function freer(target: User, me: User): boolean {
  if (me.is_admin) return false;
  if (target.is_admin) return true;
  if (me.max_age !== null && (target.max_age === null || target.max_age > me.max_age)) return true;
  return !me.all_libraries && (target.all_libraries || target.library_ids.some((id) => !me.library_ids.includes(id)));
}

/**
 * What switching to `target` asks for (`locked`: nothing would do, until it
 * gets a PIN). A profile that has to set up two-step sign-in, which the
 * server requires, takes its password whatever else it has: that sets it up.
 */
export function challenge(target: User, me: User, server?: ServerStatus): "none" | "pin" | "password" | "setup" | "locked" {
  if (target.id === me.id) return "none";
  if (mustSetUpTwoStep(target, server)) return "setup";
  if (target.has_pin) return "pin";
  if (freer(target, me)) return target.has_password ? "password" : "locked";
  return "none";
}

/** The server explains 403 (wrong PIN or password) and 429 (too many tries). */
function switchError(err: unknown): string {
  return isApiError(err, 0) ? translate("profiles.cantReach", { ns: "pages" }) : errorMessage(err);
}

function PasswordPrompt({
  target,
  setUp,
  busy,
  error,
  onSubmit,
  onCancel,
}: {
  target: User;
  /** It has to set up two-step sign-in, which its password does next. */
  setUp: boolean;
  busy: boolean;
  error: string | null;
  /** With the code from the profile's authenticator app, when it has two-step sign-in. */
  onSubmit: (password: string, code?: string) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation(["pages", "common"]);
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const ready = !!password && (!target.two_step || !!codeKind(code));
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (ready) onSubmit(password, target.two_step ? code.trim() : undefined);
  };
  return (
    <form onSubmit={submit} className="flex w-full max-w-xs flex-col items-center">
      <Avatar user={target} size="lg" />
      <div className="mt-3 text-lg font-semibold">{target.name}</div>
      <p className="mt-1 text-center text-sm text-muted">
        {setUp
          ? t("profiles.mustSetUp", { name: target.name })
          : `${target.is_admin ? t("profiles.adminProfile") : t("profiles.seesMore")} ${
              target.two_step ? t("profiles.enterPasswordAndCode") : t("profiles.enterPassword")
            }`}
      </p>
      <TextInput
        autoFocus
        type="password"
        autoComplete="current-password"
        aria-label={t("fields.password")}
        className="mt-5"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
      />
      {target.two_step ? (
        <TextInput
          autoComplete="one-time-code"
          autoCapitalize="off"
          spellCheck={false}
          aria-label={t("profiles.codeFromApp")}
          placeholder={t("profiles.codeFromApp")}
          className="mt-2"
          value={code}
          onChange={(e) => setCode(e.target.value)}
        />
      ) : null}
      <p role="alert" className="mt-2 min-h-5 text-sm text-danger">
        {error}
      </p>
      <div className="mt-2 flex w-full gap-2">
        <Button variant="ghost" className="flex-1" onClick={onCancel} disabled={busy}>
          {t("common:actions.cancel")}
        </Button>
        <Button type="submit" variant="primary" className="flex-1" loading={busy} disabled={!ready}>
          {t("profiles.switch")}
        </Button>
      </div>
    </form>
  );
}

/**
 * Setting up two-step sign-in for the profile being switched to, with the
 * password typed for it (the secret came with the server's answer), then its
 * recovery codes.
 */
type SettingUp = { kind: "scan"; password: string; setup: TwoStepSetup } | { kind: "codes"; codes: string[]; username: string };

/** "Who's watching?": pick a home profile for this device. */
export function ProfilesPage() {
  const { t } = useTranslation(["pages", "common"]);
  useDocumentTitle(t("profiles.title"));
  const me = useAuth();
  const server = useServer();
  const [params] = useSearchParams();
  const next = safeNext(params.get("next"));
  const navigate = useNavigate();
  const profiles = useProfiles();
  const switchProfile = useSwitchProfile();
  const audio = useOptionalAudioPlayer();
  const [target, setTarget] = useState<User | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [failures, setFailures] = useState(0);
  const [pinLocked, setPinLocked] = useState(false);
  const [settingUp, setSettingUp] = useState<SettingUp | null>(null);

  const doSwitch = (user: User, secret: { pin?: string; password?: string; code?: string; setup_id?: string } = {}) => {
    setError(null);
    switchProfile.mutate(
      { user_id: user.id, ...secret },
      {
        onSuccess: (res) => {
          audio?.close();
          // It set up two-step sign-in on the way: its recovery codes first.
          if (res.recovery_codes?.length) setSettingUp({ kind: "codes", codes: res.recovery_codes, username: res.user.username });
          else void navigate(next, { replace: true });
        },
        onError: (err) => {
          const setup = twoStepSetupOf(err);
          if (setup && secret.password) {
            setSettingUp({ kind: "scan", password: secret.password, setup });
            // Asking to set it up isn't a mistake; a wrong code is.
            if (secret.code) setError(errorMessage(err));
            return;
          }
          // A profile that has a PIN since the list was loaded: its pad.
          if (isApiError(err) && err.code === "wrong_pin" && !secret.pin) {
            void profiles.refetch();
            setTarget({ ...user, has_pin: true });
            return;
          }
          setError(switchError(err));
          setFailures((n) => n + 1);
          if (isApiError(err) && err.code === "pin_locked") setPinLocked(true);
        },
      },
    );
  };

  const choose = (user: User) => {
    const needs = challenge(user, me, server.data);
    if (user.id === me.id) return navigate(next, { replace: true });
    setError(null);
    setPinLocked(false);
    if (needs === "locked") setError(t("profiles.locked", { name: user.name }));
    else if (needs === "none") doSwitch(user);
    else setTarget(user);
  };

  const cancel = () => {
    setTarget(null);
    setSettingUp(null);
    setError(null);
    setPinLocked(false);
  };

  const list = profiles.data ?? [];
  const needs = target ? challenge(target, me, server.data) : "none";

  return (
    <div className="relative flex min-h-screen flex-col items-center justify-center overflow-hidden px-4 py-12">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 opacity-60"
        style={{
          background:
            "radial-gradient(900px 500px at 50% -10%, color-mix(in oklab, var(--color-accent) 16%, transparent), transparent 60%)",
        }}
      />
      <div className="relative flex w-full max-w-4xl animate-pop-in flex-col items-center">
        <Logo className="mb-10 h-9" />
        {settingUp?.kind === "codes" ? (
          <div className="w-full max-w-md text-left">
            <h1 className="mb-4 text-xl font-semibold tracking-tight">{t("profiles.saveCodes")}</h1>
            <RecoveryCodesView
              codes={settingUp.codes}
              username={settingUp.username}
              serverName={server.data?.name}
              onDone={() => void navigate(next, { replace: true })}
            />
          </div>
        ) : target && settingUp?.kind === "scan" ? (
          <div className="w-full max-w-md text-left">
            <h1 className="text-xl font-semibold tracking-tight">{t("profiles.setUpFor", { name: target.name })}</h1>
            <p className="mb-5 mt-1.5 text-sm text-muted">{t("profiles.setUpHint", { name: target.name })}</p>
            <ScanSetup
              key={settingUp.setup.secret}
              appsHint
              setup={settingUp.setup}
              busy={switchProfile.isPending}
              error={error}
              submitLabel={t("profiles.turnOnAndSwitch")}
              onCode={(code) => doSwitch(target, { password: settingUp.password, code, setup_id: settingUp.setup.id })}
              onCancel={cancel}
            />
          </div>
        ) : target && needs === "pin" ? (
          <PinPad
            // Every failed attempt starts the pad over (clears the dots).
            key={failures}
            target={target}
            busy={switchProfile.isPending}
            locked={pinLocked}
            error={error}
            onSubmit={(pin) => doSwitch(target, { pin })}
            onCancel={cancel}
          />
        ) : target && (needs === "password" || needs === "setup") ? (
          <PasswordPrompt
            target={target}
            setUp={needs === "setup"}
            busy={switchProfile.isPending}
            error={error}
            onSubmit={(password, code) => doSwitch(target, code ? { password, code } : { password })}
            onCancel={cancel}
          />
        ) : (
          <>
            <h1 className="text-3xl font-bold tracking-tight sm:text-4xl">{t("profiles.title")}</h1>
            {profiles.isPending ? (
              <Spinner className="mt-12 size-8" />
            ) : profiles.isError ? (
              <div role="alert" className="mt-10 flex flex-col items-center text-center">
                <p className="font-semibold text-fg">{t("profiles.cantLoad")}</p>
                <p className="mt-1.5 max-w-md text-sm text-muted">{errorMessage(profiles.error)}</p>
                <Button
                  className="mt-5" onClick={() => void profiles.refetch()} loading={profiles.isFetching}>
                  {t("common:actions.retry")}
                </Button>
              </div>
            ) : (
              <ul className="mt-10 flex flex-wrap justify-center gap-6 sm:gap-10">
                {list.map((p) => (
                  <li key={p.id}>
                    <button
                      type="button"
                      onClick={() => void choose(p)}
                      disabled={switchProfile.isPending}
                      className="group flex w-28 flex-col items-center gap-3 rounded-xl outline-none sm:w-32"
                    >
                      <span
                        className={cn(
                          "relative rounded-full ring-offset-4 ring-offset-bg transition group-hover:ring-4 group-hover:ring-accent group-focus-visible:ring-4 group-focus-visible:ring-accent",
                          p.id === me.id && "ring-2 ring-accent/60",
                        )}
                      >
                        <Avatar user={p} size="xl" />
                        {p.has_pin ? (
                          <span className="absolute bottom-1 right-1 flex size-7 items-center justify-center rounded-full bg-bg text-muted shadow">
                            <Lock className="size-3.5" />
                          </span>
                        ) : null}
                      </span>
                      <span className="max-w-full truncate text-sm font-medium text-muted transition-colors group-hover:text-fg">
                        {p.name}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            {error ? (
              <p role="alert" className="mt-6 text-sm text-danger">
                {error}
              </p>
            ) : null}
            <Button variant="ghost" className="mt-10" onClick={() => void navigate(next, { replace: true })}>
              {t("profiles.continueAs", { name: me.name })}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
