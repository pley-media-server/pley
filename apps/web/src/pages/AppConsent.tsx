import { useEffect, useId, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Trans, useTranslation } from "react-i18next";
import { Check, ShieldCheck, TriangleAlert } from "lucide-react";
import { errorMessage, isApiError } from "@/api/client";
import { useLogout, useProfiles, useServer } from "@/api/hooks";
import type { AppRedirect, ConsentRequest, OAuthClient } from "@/api/oauth";
import { useOptionalAudioPlayer } from "@/audio/AudioPlayer";
import { useAuth } from "@/auth";
import { Avatar } from "@/components/Avatar";
import { Badge, Button } from "@/components/ui";
import { t as translate } from "@/i18n";
import { cn } from "@/lib/cn";
import { AuthLayout } from "./AuthLayout";

/** The scopes Pley has words for (`consent.scopes`): what each lets an app do, for the profile it acts as. */
const SCOPES = ["library", "play", "playlists", "profiles", "requests", "approvals", "activity"] as const;

function knownScope(scope: string): scope is (typeof SCOPES)[number] {
  return (SCOPES as readonly string[]).includes(scope);
}

/** The administrator rights the person may give or not (`consent.optional`), each with a checkbox: managing the server. */
const OPTIONAL = ["server"] as const;

function knownOptional(scope: string): scope is (typeof OPTIONAL)[number] {
  return (OPTIONAL as readonly string[]).includes(scope);
}

/** Where an optional scope is listed: in `OPTIONAL`'s order, any Pley has no words for last. */
function order(scope: string): number {
  const at = (OPTIONAL as readonly string[]).indexOf(scope);
  return at < 0 ? OPTIONAL.length : at;
}

/** How long the page must have been seen before Allow can be pressed. */
const STEADY_MS = 1000;
/** How long after the question shows before a key can answer it. */
const KEYS_SETTLE_MS = 500;

/** pley's own apps by the name pley gave them; others by their address, which is all pley can vouch for. */
export function appName(client: OAuthClient): string {
  return client.first_party && client.name ? client.name : client.host;
}

/**
 * Whether the page has been visible and focused for `ms` without a break.
 * Leaving the window or tab starts it over: a click meant for another page
 * (a double click whose second half lands here) can't allow an app.
 */
function useSteadyAttention(ms: number): boolean {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let timer: number | undefined;
    const start = () => {
      if (document.visibilityState === "visible" && document.hasFocus()) timer = window.setTimeout(() => setReady(true), ms);
    };
    const restart = () => {
      window.clearTimeout(timer);
      timer = undefined;
      setReady(false);
      start();
    };
    start();
    window.addEventListener("focus", restart);
    window.addEventListener("blur", restart);
    document.addEventListener("visibilitychange", restart);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("focus", restart);
      window.removeEventListener("blur", restart);
      document.removeEventListener("visibilitychange", restart);
    };
  }, [ms]);
  return ready;
}

/**
 * Whether `ms` went by since the question showed. Until then a key press
 * was meant for what was there before: on /link, Enter sends the code, and
 * pressed twice or held down, it would land on the focused "Don't allow".
 */
function useKeysSettled(ms: number) {
  const settled = useRef(false);
  useEffect(() => {
    const timer = window.setTimeout(() => {
      settled.current = true;
    }, ms);
    return () => window.clearTimeout(timer);
  }, [ms]);
  return settled;
}

/** Where an app's answer goes back to, in words. */
export function whereItGoes(redirect: AppRedirect): string {
  return redirect.kind === "web"
    ? translate("consent.website", { ns: "pages", address: redirect.to })
    : redirect.kind === "loopback"
      ? translate("consent.appOnComputer", { ns: "pages" })
      : translate("consent.appOnDevice", { ns: "pages" });
}

/** Where the answer goes, and what to be sure of before allowing. */
function Destination({ redirect }: { redirect: ConsentRequest["redirect"] }) {
  const { t } = useTranslation("pages");
  if (redirect.kind === "device") {
    const device = redirect.device ?? t("consent.aDevice");
    return (
      <div>
        <p className="font-medium">{t("consent.goesTo")}</p>
        <p className="mt-1">
          <Trans
            t={t}
            i18nKey="consent.deviceShowing"
            values={{ code: redirect.to }}
            components={{ code: <span className="font-mono font-semibold tracking-wider" /> }}
          />
        </p>
        {redirect.device || redirect.address ? (
          <p className="mt-1 text-muted">
            {redirect.address ? t("consent.saysItIsAt", { device, address: redirect.address }) : t("consent.saysItIs", { device })}
          </p>
        ) : null}
        <p className="mt-1 text-danger">{t("consent.onlyIfOnScreen")}</p>
      </div>
    );
  }
  return (
    <div>
      <p className="font-medium">{t("consent.goesBackTo")}</p>
      <p className="mt-1">{whereItGoes(redirect)}</p>
      {redirect.kind !== "web" ? <p className="mt-1 text-danger">{t("consent.onlyIfStarted")}</p> : null}
    </div>
  );
}

/** One administrator right, given only when its box is ticked: the whole row is the label. */
function OptionalScope({
  scope,
  checked,
  disabled,
  onChange,
}: {
  scope: string;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  const { t } = useTranslation("pages");
  const id = useId();
  return (
    <label
      htmlFor={id}
      className={cn(
        "flex min-h-14 cursor-pointer items-start gap-3 px-3 py-3 transition-colors hover:bg-white/5",
        disabled && "cursor-default opacity-60 hover:bg-transparent",
      )}
    >
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        // Named by its label alone; the hint describes it.
        aria-labelledby={`${id}-label`}
        aria-describedby={knownOptional(scope) ? `${id}-hint` : undefined}
        onChange={(e) => onChange(e.target.checked)}
        className="peer sr-only"
      />
      <span
        aria-hidden
        className={cn(
          "mt-0.5 flex size-5 shrink-0 items-center justify-center rounded border-2 transition-colors peer-focus-visible:ring-2 peer-focus-visible:ring-accent peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-surface",
          checked ? "border-accent bg-accent text-accent-fg" : "border-border-strong",
        )}
      >
        {checked ? <Check className="size-3.5" strokeWidth={3} /> : null}
      </span>
      <span className="min-w-0">
        <span id={`${id}-label`} className="block font-medium">
          {knownOptional(scope) ? t(`consent.optional.${scope}.label`) : scope}
        </span>
        {knownOptional(scope) ? (
          <span id={`${id}-hint`} className="mt-0.5 block text-xs text-muted">
            {t(`consent.optional.${scope}.hint`)}
          </span>
        ) : null}
      </span>
    </label>
  );
}

/**
 * Asks whether an app may use the account: an app that sent the browser to
 * the authorize page, or a TV that showed a code. It shows only what the
 * server answered, as text. Deny has the focus, and Allow waits until the
 * page has been seen for a whole second. A key held down, or pressed right
 * as the question shows, answers nothing: a refusal can't be taken back.
 *
 * Managing the server, when an app asks for it, is a box the person ticks:
 * the app works without it, and it isn't given unless ticked here. The
 * server offers it only to an administrator, and lists approving requests,
 * which Pley's own apps need on an administrator's account, with the rest.
 */
export function AppConsent({
  request,
  returnTo,
  onAnswer,
}: {
  request: ConsentRequest;
  /** This page, with what it needs to start again: "Use another profile" and "Not you? Sign out" come back to it. */
  returnTo: string;
  /** Sends the answer, with the scopes allowed; the page takes over once it is in. A 404 means the request is over. */
  onAnswer: (allow: boolean, scopes: string[]) => Promise<unknown>;
}) {
  const { t } = useTranslation("pages");
  const user = useAuth();
  const server = useServer();
  const profiles = useProfiles(user.home);
  const logout = useLogout();
  const audio = useOptionalAudioPlayer();
  const navigate = useNavigate();
  const ready = useSteadyAttention(STEADY_MS);
  const keysSettled = useKeysSettled(KEYS_SETTLE_MS);
  const sent = useRef(false);
  /** The button pressed, while its answer is on its way. */
  const [deciding, setDeciding] = useState<"allow" | "deny" | null>(null);
  const [error, setError] = useState<unknown>(null);
  /** The administrator rights ticked: none until the person ticks them. */
  const [ticked, setTicked] = useState<ReadonlySet<string>>(() => new Set());

  const { client, redirect } = request;
  const app = appName(client);
  const optional = request.optional;
  const required = request.scopes.filter((scope) => !optional.includes(scope));
  const offered = [...optional].sort((a, b) => order(a) - order(b));

  const answer = (allow: boolean) => {
    if (sent.current || (allow && !ready)) return;
    sent.current = true;
    setDeciding(allow ? "allow" : "deny");
    setError(null);
    const scopes = allow ? request.scopes.filter((scope) => !optional.includes(scope) || ticked.has(scope)) : [];
    onAnswer(allow, scopes).catch((err: unknown) => {
      // 404: the request is over; anything else may be tried again.
      sent.current = isApiError(err, 404);
      setDeciding(null);
      setError(err);
    });
  };

  const over = isApiError(error, 404);
  const canSwitch = (profiles.data?.length ?? 0) > 1;

  return (
    <AuthLayout title={t("consent.title", { app })} subtitle={server.data ? t("consent.onServer", { server: server.data.name }) : undefined}>
      <div className="space-y-6 text-sm">
        {client.first_party ? (
          <Badge tone="accent">
            <ShieldCheck className="size-3.5" aria-hidden /> {t("consent.firstParty")}
          </Badge>
        ) : (
          <div className="flex gap-2.5 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-fg/90">
            <TriangleAlert className="mt-0.5 size-4 shrink-0 text-danger" aria-hidden />
            <div className="space-y-1">
              {client.name ? <p>{t("consent.callsItself", { name: client.name })}</p> : null}
              <p>{t("consent.notOwn")}</p>
            </div>
          </div>
        )}

        <div className="flex items-center gap-3 rounded-lg bg-surface-2 px-3 py-2.5">
          <Avatar user={user} size="md" />
          <div className="min-w-0 flex-1">
            <div className="truncate font-semibold">{user.name}</div>
            <div className="flex flex-wrap gap-x-3 text-xs">
              {canSwitch ? (
                <button
                  type="button"
                  className="font-medium text-accent hover:text-accent-hover"
                  onClick={() => void navigate(`/profiles?next=${encodeURIComponent(returnTo)}`)}
                >
                  {t("consent.otherProfile")}
                </button>
              ) : null}
              <button
                type="button"
                className="text-muted hover:text-fg"
                disabled={logout.isPending}
                onClick={() => {
                  audio?.close();
                  logout.mutate(undefined, {
                    onSettled: () => void navigate(`/login?next=${encodeURIComponent(returnTo)}`, { replace: true }),
                  });
                }}
              >
                {t("consent.notYou")}
              </button>
            </div>
          </div>
        </div>

        <div>
          <p className="font-medium">{t("consent.willBeAble")}</p>
          <ul className="mt-2 space-y-1.5">
            {required.map((scope) => (
              <li key={scope} className="flex gap-2">
                <Check className="mt-0.5 size-4 shrink-0 text-success" aria-hidden />
                <span>
                  {scope === "activity"
                    ? // Everyone's streams only ever go to Pley's own apps on an administrator's account: others see and stop the profile's own.
                      t(client.first_party && user.is_admin ? "consent.scopes.activityEveryone" : "consent.scopes.activity", {
                        profile: user.name,
                      })
                    : knownScope(scope)
                      ? t(`consent.scopes.${scope}`, { profile: user.name })
                      : scope}
                </span>
              </li>
            ))}
          </ul>
        </div>

        {offered.length > 0 ? (
          <fieldset>
            <legend className="flex w-full items-baseline justify-between gap-3">
              <span className="font-medium">{t("consent.adminAccess")}</span>
              <span className="text-xs text-muted">{t("consent.optionalLabel")}</span>
            </legend>
            <div className="mt-2 divide-y divide-border overflow-hidden rounded-lg border border-border">
              {offered.map((scope) => (
                <OptionalScope
                  key={scope}
                  scope={scope}
                  checked={ticked.has(scope)}
                  disabled={deciding !== null}
                  onChange={(on) =>
                    setTicked((before) => {
                      const next = new Set(before);
                      if (on) next.add(scope);
                      else next.delete(scope);
                      return next;
                    })
                  }
                />
              ))}
            </div>
          </fieldset>
        ) : null}

        <p className="text-muted">{t(user.is_admin ? "consent.cantChangeAdmin" : "consent.cantChange")}</p>

        <Destination redirect={redirect} />

        {over ? (
          <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-danger">
            {redirect.kind === "device" ? t("consent.overTv") : t("consent.overApp")}
          </p>
        ) : (
          <>
            {error ? (
              <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-danger">
                {errorMessage(error)}
              </p>
            ) : null}
            <div className="flex gap-2">
              <Button
                variant="outline"
                className="flex-1"
                autoFocus
                loading={deciding === "deny"}
                disabled={deciding !== null}
                onKeyDown={(e) => {
                  // Enter and Space press the button, but not held down, or right as the question shows.
                  if ((e.key === "Enter" || e.key === " ") && (e.repeat || !keysSettled.current)) e.preventDefault();
                }}
                onClick={() => answer(false)}
              >
                {t("consent.deny")}
              </Button>
              <Button
                variant="primary"
                className="flex-1"
                loading={deciding === "allow"}
                disabled={!ready || deciding !== null}
                onClick={() => answer(true)}
              >
                {t("consent.allow")}
              </Button>
            </div>
          </>
        )}
      </div>
    </AuthLayout>
  );
}
