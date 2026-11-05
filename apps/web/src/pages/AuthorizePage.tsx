import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { useLocation } from "react-router";
import { useTranslation } from "react-i18next";
import { errorMessage, isApiError } from "@/api/client";
import { decide, startRequest, type ConsentRequest, type RequestAnswer, type RequestError } from "@/api/oauth";
import { Splash } from "@/auth";
import { Button, Spinner } from "@/components/ui";
import { useDocumentTitle } from "@/lib/hooks";
import { AppConsent, appName, whereItGoes } from "./AppConsent";
import { AuthLayout } from "./AuthLayout";

/** How long before a link back to the app shows, in case the browser didn't go. */
const FALLBACK_MS = 3000;

function subscribeVisibility(onChange: () => void) {
  document.addEventListener("visibilitychange", onChange);
  return () => document.removeEventListener("visibilitychange", onChange);
}

function useVisible(): boolean {
  return useSyncExternalStore(subscribeVisibility, () => document.visibilityState === "visible");
}

/**
 * The authorization endpoint: an app sent the browser here to ask for the
 * account. The server checks the request; this page only shows what it
 * answers, as text, and never goes anywhere without a click.
 */
export function AuthorizePage() {
  const { t } = useTranslation("pages");
  useDocumentTitle(t("authorize.documentTitle"));
  const { search } = useLocation();
  const query = search.replace(/^\?/, "");
  // One request per page (React runs effects twice in development).
  const started = useRef<{ query: string; answer: Promise<RequestAnswer> } | null>(null);
  const [result, setResult] = useState<{ query: string; answer?: RequestAnswer; error?: unknown } | null>(null);

  useEffect(() => {
    if (started.current?.query !== query) started.current = { query, answer: startRequest(query) };
    let live = true;
    started.current.answer.then(
      (answer) => live && setResult({ query, answer }),
      (error: unknown) => live && setResult({ query, error }),
    );
    return () => {
      live = false;
    };
  }, [query]);

  const current = result?.query === query ? result : null;
  // 401: the app goes to sign-in and comes back.
  if (!current || isApiError(current.error, 401)) return <Splash />;
  if (!current.answer) {
    return (
      <AuthLayout title={isApiError(current.error, 400) ? t("authorize.cantSignIn") : t("authorize.cantLoad")}>
        <p className="text-sm text-muted">{errorMessage(current.error)}</p>
      </AuthLayout>
    );
  }
  if (current.answer.status === "error") return <RequestErrorScreen request={current.answer} />;
  return <Consent request={current.answer} />;
}

/**
 * A mistake the app made: it hears about it only if the person goes back,
 * knowing where that is (a request with a mistake can be a lure to a site).
 */
function RequestErrorScreen({ request }: { request: RequestError }) {
  const { t } = useTranslation("pages");
  return (
    <AuthLayout title={t("authorize.cantAllow")} subtitle={t("authorize.fromApp", { app: appName(request.client) })}>
      <div className="space-y-5 text-sm">
        <p className="text-muted">{errorMessage(new Error(request.message))}</p>
        <div>
          <p className="font-medium">{t("authorize.backGoesTo")}</p>
          <p className="mt-1">{whereItGoes(request.redirect)}</p>
        </div>
        <Button variant="primary" className="w-full" onClick={() => window.location.assign(request.back_url)}>
          {t("authorize.back")}
        </Button>
      </div>
    </AuthLayout>
  );
}

/** Where the way back to an app leads once the request is over (a link followed late, from a tab left open). */
export function RequestOverPage() {
  const { t } = useTranslation("pages");
  useDocumentTitle(t("authorize.overDocumentTitle"));
  return (
    <AuthLayout title={t("authorize.over")}>
      <p className="text-sm text-muted">{t("authorize.startAgain")}</p>
    </AuthLayout>
  );
}

/** Consent, then back to the app with the answer, at once and inside the click. */
function Consent({ request }: { request: ConsentRequest }) {
  const { pathname, search } = useLocation();
  const [continueUrl, setContinueUrl] = useState<string | null>(null);

  if (continueUrl) return <Returning app={appName(request.client)} url={continueUrl} />;
  return (
    <AppConsent
      request={request}
      returnTo={pathname + search}
      onAnswer={(allow) =>
        decide(request.id, allow).then(({ continue_url }) => {
          window.location.assign(continue_url);
          setContinueUrl(continue_url);
        })
      }
    />
  );
}

/** The browser is on its way back to the app; a link in case it didn't go. */
function Returning({ app, url }: { app: string; url: string }) {
  const { t } = useTranslation("pages");
  const visible = useVisible();
  const [late, setLate] = useState(false);
  useEffect(() => {
    const timer = window.setTimeout(() => setLate(true), FALLBACK_MS);
    return () => window.clearTimeout(timer);
  }, []);
  return (
    <AuthLayout title={t("authorize.returning", { app })}>
      {late && visible ? (
        <a href={url} className="text-sm font-medium text-accent hover:text-accent-hover">
          {t("authorize.open", { app })}
        </a>
      ) : (
        <Spinner />
      )}
    </AuthLayout>
  );
}
