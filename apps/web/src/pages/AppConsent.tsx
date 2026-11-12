import { useEffect, useRef, useState } from "react";
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
import { currentLocale, t as translate } from "@/i18n";
import { AuthLayout } from "./AuthLayout";

/** The scopes Pley has words for (`consent.scopes`): what each lets an app do, for the profile it acts as. */
const SCOPES = ["library", "play", "playlists", "profiles", "requests", "approvals", "activity", "server"] as const;

function knownScope(scope: string): scope is (typeof SCOPES)[number] {
  return (SCOPES as readonly string[]).includes(scope);
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

/**
 * Asks whether an app may use the account: an app that sent the browser to
 * the authorize page, or a TV that showed a code. It shows only what the
 * server answered, as text. Deny has the focus, and Allow waits until the
 * page has been seen for a whole second. A key held down, or pressed right
 * as the question shows, answers nothing: a refusal can't be taken back.
 */
export function AppConsent({
  request,
  returnTo,
  onAnswer,
}: {
  request: ConsentRequest;
  /** This page, with what it needs to start again: "Use another profile" and "Not you? Sign out" come back to it. */
  returnTo: string;
  /** Sends the answer; the page takes over once it is in. A 404 means the request is over. */
  onAnswer: (allow: boolean) => Promise<unknown>;
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

  const { client, redirect } = request;
  const app = appName(client);

  const answer = (allow: boolean) => {
    if (sent.current || (allow && !ready)) return;
    sent.current = true;
    setDeciding(allow ? "allow" : "deny");
    setError(null);
    onAnswer(allow).catch((err: unknown) => {
      // 404: the request is over; anything else may be tried again.
      sent.current = isApiError(err, 404);
      setDeciding(null);
      setError(err);
    });
  };

  const over = isApiError(error, 404);
  const canSwitch = (profiles.data?.length ?? 0) > 1;
  // The administrator rights an app can get: managing requests, and for one
  // of Pley's own seeing and stopping everyone's streams, which managing the
  // server includes.
  const approves = user.is_admin && request.scopes.includes("approvals");
  // Managing the server: Pley's own apps only (the server refuses it to others).
  const manages = user.is_admin && client.first_party && request.scopes.includes("server");
  const oversees = user.is_admin && client.first_party && request.scopes.includes("activity");
  const rights = new Intl.ListFormat(currentLocale(), { type: "conjunction" }).format(
    [approves ? t("consent.rights.approvals") : null, oversees || manages ? t("consent.rights.activity") : null].filter(
      (right) => right !== null,
    ),
  );

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
            {request.scopes.map((scope) => (
              <li key={scope} className="flex gap-2">
                <Check className="mt-0.5 size-4 shrink-0 text-success" aria-hidden />
                <span>
                  {scope === "activity"
                    ? // Everyone's streams only ever go to Pley's own apps: another's sees and stops the profile's own.
                      t(client.first_party ? "consent.scopes.activityEveryone" : "consent.scopes.activity", { profile: user.name })
                    : knownScope(scope)
                      ? t(`consent.scopes.${scope}`, { profile: user.name })
                      : scope}
                </span>
              </li>
            ))}
          </ul>
          <p className="mt-3 text-muted">{rights || manages ? t("consent.cantChange") : t("consent.cantChangeNoAdmin")}</p>
          {user.is_admin ? (
            <p className="mt-1 text-muted">
              {manages
                ? `${t("consent.managesServer")} ${t("consent.managesServerToo", { rights })}`
                : rights
                  ? t("consent.rightsOnly", { rights })
                  : t("consent.adminStays")}
            </p>
          ) : null}
        </div>

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
