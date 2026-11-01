import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Check, ShieldCheck, TriangleAlert } from "lucide-react";
import { errorMessage, isApiError } from "@/api/client";
import { useLogout, useProfiles, useServer } from "@/api/hooks";
import type { AppRedirect, ConsentRequest, OAuthClient } from "@/api/oauth";
import { useOptionalAudioPlayer } from "@/audio/AudioPlayer";
import { useAuth } from "@/auth";
import { Avatar } from "@/components/Avatar";
import { Badge, Button } from "@/components/ui";
import { AuthLayout } from "./AuthLayout";

/** What each scope lets an app do, for the profile it acts as. */
const SCOPE_WORDS: Record<string, (profile: string) => string> = {
  library: (p) => `See ${p}'s libraries, playlists and watch history.`,
  play: (p) => `Play titles, and keep ${p}'s place, watched marks, ratings, track choices and library order.`,
  playlists: (p) => `Make, change and delete ${p}'s playlists.`,
  profiles: () => "Switch to the household's other profiles. Profiles with a PIN still ask for it.",
  requests: (p) => `Ask for movies and shows the libraries don't have, and follow ${p}'s requests.`,
  approvals: () => "Approve and decline what people ask for, where the account is an administrator.",
};

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
    ? `the website ${redirect.to}`
    : redirect.kind === "loopback"
      ? "an app on this computer"
      : "an app on this device";
}

/** Where the answer goes, and what to be sure of before allowing. */
function Destination({ redirect }: { redirect: ConsentRequest["redirect"] }) {
  if (redirect.kind === "device") {
    return (
      <div>
        <p className="font-medium">It goes to:</p>
        <p className="mt-1">
          the TV or device showing{" "}
          <span className="font-mono font-semibold tracking-wider">{redirect.to}</span>
        </p>
        {redirect.device || redirect.address ? (
          <p className="mt-1 text-muted">
            It says it is {redirect.device ?? "a device"}
            {redirect.address ? `, at ${redirect.address}` : ""}.
          </p>
        ) : null}
        <p className="mt-1 text-danger">
          Only allow this if that code is on a screen in front of you right now. Never use a code someone sent you.
        </p>
      </div>
    );
  }
  return (
    <div>
      <p className="font-medium">It goes back to:</p>
      <p className="mt-1">{whereItGoes(redirect)}</p>
      {redirect.kind !== "web" ? (
        <p className="mt-1 text-danger">Only allow this if you just started signing in to the app.</p>
      ) : null}
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
  // The one administrator right an app can get: managing requests.
  const approves = user.is_admin && request.scopes.includes("approvals");

  return (
    <AuthLayout title={`${app} wants to use your Pley account`} subtitle={server.data ? `on ${server.data.name}` : undefined}>
      <div className="space-y-6 text-sm">
        {client.first_party ? (
          <Badge tone="accent">
            <ShieldCheck className="size-3.5" aria-hidden /> Pley's own app
          </Badge>
        ) : (
          <div className="flex gap-2.5 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-fg/90">
            <TriangleAlert className="mt-0.5 size-4 shrink-0 text-danger" aria-hidden />
            <div className="space-y-1">
              {client.name ? <p>It calls itself “{client.name}” (Pley can't check that name)</p> : null}
              <p>This app isn't one of Pley's own</p>
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
                  Use another profile
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
                Not you? Sign out
              </button>
            </div>
          </div>
        </div>

        <div>
          <p className="font-medium">It will be able to:</p>
          <ul className="mt-2 space-y-1.5">
            {request.scopes.map((scope) => (
              <li key={scope} className="flex gap-2">
                <Check className="mt-0.5 size-4 shrink-0 text-success" aria-hidden />
                <span>{SCOPE_WORDS[scope]?.(user.name) ?? scope}</span>
              </li>
            ))}
          </ul>
          <p className="mt-3 text-muted">
            {approves
              ? "It can't change your password, sign-in settings or devices."
              : "It can't change your password, sign-in settings or devices, and it never gets administrator rights."}
          </p>
          {user.is_admin ? (
            <p className="mt-1 text-muted">
              {approves
                ? "Of your administrator rights it gets only approving and declining requests: the others stay in Pley's web app."
                : "Your administrator rights stay in Pley's web app."}
            </p>
          ) : null}
        </div>

        <Destination redirect={redirect} />

        {over ? (
          <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-danger">
            {redirect.kind === "device"
              ? "This request is over: start again on your TV"
              : "This request is over: start again from the app"}
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
                Don't allow
              </Button>
              <Button
                variant="primary"
                className="flex-1"
                loading={deciding === "allow"}
                disabled={!ready || deciding !== null}
                onClick={() => answer(true)}
              >
                Allow
              </Button>
            </div>
          </>
        )}
      </div>
    </AuthLayout>
  );
}
