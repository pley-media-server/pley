import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import type { ComponentChildren } from "preact";
import { BACKOFF_MS, DeviceSignIn, type DeviceCode } from "../../../auth/device";
import { AuthError, UnreachableError } from "../../../auth/metadata";
import { probe } from "../../../auth/servers";
import { t, Trans } from "../../../i18n";
import { useUi } from "../../context";
import { Pill, TextButton } from "../../kit/buttons";
import { G } from "../../kit/focusable";
import { Icon } from "../../kit/icons";
import { sentence, useApp } from "../common";
import { BrandGlow, LanguageGlobe, LanguagePanel, PleyLogo, QrCode, StatusLine } from "./kit";

/** Where signing in with a code stands (the Android app's LinkStatus). */
export type LinkStatus =
  | { kind: "starting" }
  | { kind: "waiting" }
  | { kind: "unreachable"; retryAt: number }
  | { kind: "declined" }
  | { kind: "expired" }
  | { kind: "failed"; message: string }
  | { kind: "signed_in"; name: string | null };

/** Codes renewed without a word before "Code expired" shows (about 40 minutes of 10-minute codes). */
export const RENEWALS = 3;
/** How long "Signed in as …" shows before Home. */
export const SIGNED_IN_MS = 500;
/** How long the name may take to arrive. */
export const NAME_WAIT_MS = 3_000;

/** The code as shown: upper case, letters and digits; 8 of them as "XXXX-XXXX". */
export function displayCode(code: string): string {
  const clean = code.toUpperCase().replace(/[^A-Z0-9]/g, "");
  return clean.length === 8 ? `${clean.slice(0, 4)}-${clean.slice(4)}` : code;
}

/** The code's two halves when it splits on "-" into two, else the whole. */
export function codeHalves(code: string): string[] {
  const parts = code.split("-");
  return parts.length === 2 && parts[0] && parts[1] ? parts : [code];
}

/** The address to type: the verification URI without its scheme ("192.168.1.20:7539/link"). */
export function typedAddress(uri: string): string {
  return uri.replace(/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//, "");
}

/**
 * Whether `server` requires two-step sign-in (GET /api/server), once it
 * answers. Signing in on the phone sets it up, so this only says so,
 * under the title.
 */
function useTwoStepRequired(server: string): boolean {
  const app = useApp();
  const [required, setRequired] = useState(false);
  useEffect(() => {
    let alive = true;
    probe(app.http, server).then(
      (info) => alive && setRequired(!!info?.twoStepRequired),
      () => undefined,
    );
    return () => void (alive = false);
  }, [server]);
  return required;
}

/**
 * Signing in with a code (the Android app's LinkViewModel) for one screen:
 * a code from the server, polled while the TV shows the app; three expired
 * codes renewed quietly; the server unreachable retried by itself. On
 * approval the tokens are kept and `onApproved` decides what follows.
 */
function useLink(server: string, onApproved: () => Promise<string | null>) {
  const app = useApp();
  const [status, setStatus] = useState<LinkStatus>({ kind: "starting" });
  const [code, setCode] = useState<DeviceCode | null>(null);
  const run = useRef(0);
  const renewals = useRef(0);
  const failures = useRef(0);
  const codeRef = useRef<DeviceCode | null>(null);
  const statusRef = useRef<LinkStatus>(status);
  const set = (s: LinkStatus) => {
    statusRef.current = s;
    setStatus(s);
  };
  const signIn = useRef(new DeviceSignIn(app.http, app.platform.clientId));

  const stop = () => {
    run.current++;
  };

  const fetchCode = (silent: boolean) => {
    const mine = ++run.current;
    if (!silent) set({ kind: "starting" });
    signIn.current.start(server).then(
      (fresh) => {
        if (mine !== run.current) return;
        failures.current = 0;
        codeRef.current = fresh;
        setCode(fresh);
        void poll(fresh, mine);
      },
      (e) => {
        if (mine !== run.current) return;
        if (e instanceof UnreachableError) {
          const wait = BACKOFF_MS[Math.min(failures.current, BACKOFF_MS.length - 1)]!;
          failures.current++;
          set({ kind: "unreachable", retryAt: Date.now() + wait });
          setTimeout(() => mine === run.current && fetchCode(true), wait);
        } else {
          set({ kind: "failed", message: e instanceof AuthError ? e.message : String((e as Error)?.message ?? e) });
        }
      },
    );
  };

  const poll = async (c: DeviceCode, mine: number) => {
    const stopped = () => mine !== run.current;
    const result = await signIn.current.poll(
      c,
      (u) => {
        if (stopped()) return;
        if (u.kind === "waiting") set({ kind: "waiting" });
        else if (u.kind === "unreachable") set({ kind: "unreachable", retryAt: Date.now() + u.retryInMs });
      },
      stopped,
    );
    if (stopped()) return;
    switch (result.kind) {
      case "signed_in": {
        const tokens = app.tokenStore;
        try {
          if (!tokens) throw new AuthError(t("onboarding.errors.signInEnded"));
          await tokens.install(result.tokens, c.scope);
        } catch (e) {
          if (!stopped()) set({ kind: "failed", message: e instanceof Error ? e.message : String(e) });
          return;
        }
        app.signedOut.value = false;
        app.profileGatePassed = true;
        const name = await onApproved();
        if (!stopped()) set({ kind: "signed_in", name });
        return;
      }
      case "denied":
        set({ kind: "declined" });
        return;
      case "expired":
        if (renewals.current < RENEWALS) {
          renewals.current++;
          fetchCode(true);
        } else set({ kind: "expired" });
        return;
      case "failed":
        set({ kind: "failed", message: result.message });
        return;
      default:
        return;
    }
  };

  /** The screen shows: the same code polled again, or a new one if it expired meanwhile. */
  const started = () => {
    const s = statusRef.current.kind;
    if (s === "signed_in" || s === "failed" || s === "declined" || s === "expired") return;
    const c = codeRef.current;
    if (!c) fetchCode(false);
    else if (Date.now() >= c.expiresAt) {
      codeRef.current = null;
      setCode(null);
      fetchCode(false);
    } else {
      const mine = ++run.current;
      void poll(c, mine);
    }
  };

  useEffect(() => {
    if (app.visible.value) started();
    else stop();
  }, [app.visible.value]);
  useEffect(() => stop, []);

  return {
    status,
    code,
    newCode: () => {
      renewals.current = 0;
      failures.current = 0;
      fetchCode(false);
    },
  };
}

/** Sign-in with a code (TvLinkScreen): the steps, the code, a QR code of the link. */
export function LinkScreen(p: { server: string }) {
  const app = useApp();
  const ui = useUi();
  useEffect(() => {
    app.use(p.server);
    app.platform.keepAwake(true);
    return () => app.platform.keepAwake(false);
  }, []);
  const link = useLink(p.server, async () => {
    const name = await nameWithin(app, NAME_WAIT_MS, null);
    setTimeout(() => ui.nav.restart({ name: "home" }), SIGNED_IN_MS);
    return name;
  });
  const twoStep = useTwoStepRequired(p.server);
  return (
    <LinkContent
      status={link.status}
      code={link.code}
      onNewCode={link.newCode}
      intro={twoStep ? t("onboarding.link.twoStepRequired") : undefined}
      backLabel={t("onboarding.link.changeServer")}
      globe
      onBack={() => {
        app.forgetServer();
        ui.nav.restart({ name: "servers" });
      }}
    />
  );
}

/**
 * Approving a profile on a phone (Profiles › Approve): a password profile,
 * or "Sign in as another profile". Signed in again with a code, the app
 * acts as whoever the approver picked.
 */
export function ApproveContent(p: { server: string; profile: { id: number; name: string } | null; previousUser: number | null; onBack: () => void }) {
  const app = useApp();
  const ui = useUi();
  const link = useLink(p.server, async () => {
    const name = await nameWithin(app, NAME_WAIT_MS, p.profile ? p.profile.id : p.previousUser, !p.profile);
    setTimeout(() => ui.nav.restart({ name: "home" }), SIGNED_IN_MS);
    return name;
  });
  useEffect(
    () =>
      ui.input.push((key, e) => {
        if (key !== "back") return false;
        if (e.type === "down" && !e.repeat) p.onBack();
        return true;
      }),
    [],
  );
  const title = p.profile ? t("onboarding.link.approveTitle") : t("onboarding.profiles.signInAnother");
  const intro = p.profile ? t("onboarding.link.approveIntro", { name: p.profile.name }) : t("onboarding.link.anotherIntro");
  return (
    <LinkContent status={link.status} code={link.code} onNewCode={link.newCode} title={title} intro={intro} backLabel={t("onboarding.profiles.useAnother")} onBack={p.onBack} />
  );
}

/**
 * The signed-in name, waiting at most `ms` for it; with `want`, until the
 * session acts as that profile (or, `other`, as anyone but it).
 */
async function nameWithin(app: ReturnType<typeof useApp>, ms: number, want: number | null, other = false): Promise<string | null> {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    try {
      const api = app.api.value;
      if (!api) return null;
      const me = await api.me();
      const matches = want === null || (other ? me.id !== want : me.id === want);
      if (matches) {
        void app.loadAccount().catch(() => undefined);
        return me.display_name || me.name || null;
      }
    } catch {
      // Not yet.
    }
    await new Promise((r) => setTimeout(r, 400));
  }
  void app.loadAccount().catch(() => undefined);
  return null;
}

function LinkContent(p: {
  status: LinkStatus;
  code: DeviceCode | null;
  onNewCode: () => void;
  title?: string;
  intro?: string;
  backLabel: string;
  onBack: () => void;
  /** The globe at the top right, signing in (not approving a profile from Who's watching?). */
  globe?: boolean;
}) {
  const ui = useUi();
  const s = p.status;
  const needsNew = s.kind === "declined" || s.kind === "expired";
  const backRef = useRef<HTMLElement | null>(null);
  const newRef = useRef<HTMLElement | null>(null);
  const [choosingLanguage, setChoosingLanguage] = useState(false);
  // Whenever a new code is needed, its button takes focus; back to the other one after.
  useLayoutEffect(() => {
    if (choosingLanguage) return;
    const target = needsNew ? newRef.current?.querySelector<HTMLElement>("[data-f]") : backRef.current?.querySelector<HTMLElement>("[data-f]");
    if (target) ui.focus.focus(target);
  }, [needsNew, s.kind === "signed_in"]);
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (s.kind !== "unreachable") return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [s.kind]);

  if (s.kind === "signed_in") {
    return (
      <BrandGlow>
        <div class="signed-in" role="status">
          <div class="signed-in-tile">
            <Icon name="check" size={32} color="var(--bg)" />
          </div>
          <div class="t-page c-fg lines-2" style="margin-top:20px">
            {s.name ? t("onboarding.link.signedInAs", { name: s.name }) : t("onboarding.link.signedIn")}
          </div>
        </div>
      </BrandGlow>
    );
  }

  const code = p.code ? displayCode(p.code.userCode) : null;
  const halves = code ? codeHalves(code) : ["····", "····"];
  const dim = !code || s.kind === "expired" || (s.kind === "starting" && !!code);
  const address = p.code ? typedAddress(p.code.verificationUri) : "…";
  const qr = p.code ? p.code.verificationUriComplete ?? p.code.verificationUri : null;

  let line: { text: string; waiting: boolean; cls: string } | null = null;
  switch (s.kind) {
    case "starting":
      line = { text: t("onboarding.link.gettingCode"), waiting: true, cls: "t-label c-muted" };
      break;
    case "waiting":
      line = { text: t("onboarding.link.waiting"), waiting: true, cls: "t-label c-muted" };
      break;
    case "unreachable":
      line = {
        text: t("onboarding.link.retrying", { seconds: Math.max(0, Math.ceil((s.retryAt - now) / 1000)) }),
        waiting: false,
        cls: "t-label c-danger",
      };
      break;
    case "declined":
      line = { text: t("onboarding.link.declined"), waiting: false, cls: "t-label c-danger" };
      break;
    case "expired":
      line = { text: t("onboarding.link.expired"), waiting: false, cls: "t-label c-muted" };
      break;
    case "failed":
      line = { text: sentence(s.message), waiting: false, cls: "t-label c-danger" };
      break;
  }

  return (
    <BrandGlow>
      <div class="link-left">
        <PleyLogo height={40} />
        <div class="t-page c-fg one-line" style="margin-top:28px">
          {p.title ?? t("onboarding.link.title")}
        </div>
        {p.intro && (
          <div class="t-body c-muted lines-2" style="margin-top:6px">
            {p.intro}
          </div>
        )}
        <div style={`margin-top:${p.intro ? 10 : 18}px`}>
          <Step n={1}>{t("onboarding.link.scan")}</Step>
          <Step n={2}>
            <Trans k="onboarding.link.goTo" values={{ address }} tags={{ address: (text) => <span class="link-address">{text}</span> }} />
          </Step>
          <Step n={3}>{t("onboarding.link.enterCode")}</Step>
        </div>
        <div
          class={`code-row${dim ? " dim" : ""}`}
          aria-label={code ? t("onboarding.link.codeLabel", { code: halves.map((h) => h.split("").join(" ")).join(", ") }) : undefined}
        >
          <div class="code-field t-code">{halves[0]}</div>
          {halves.length > 1 && (
            <>
              <div class="code-sep">
                <div />
              </div>
              <div class="code-field t-code">{halves[1]}</div>
            </>
          )}
        </div>
        <div ref={(el) => {
          newRef.current = el;
        }} style="margin-top:16px">
          {needsNew ? (
            <G kind="row">
              <Pill label={t("onboarding.link.newCode")} icon="refresh-cw" onSelect={p.onNewCode} />
            </G>
          ) : (
            <div class="link-notes t-body c-muted">
              <div class="one-line">{t("onboarding.link.sameWifi")}</div>
              <div class="one-line" style="margin-top:6px">
                {t("onboarding.link.otherDevice")}
              </div>
            </div>
          )}
        </div>
      </div>
      {line && (
        <div class="link-status">
          <StatusLine text={line.text} waiting={line.waiting} class={line.cls} />
        </div>
      )}
      <div class="link-right">
        <div class={`qr-tile${s.kind === "expired" ? " dim" : ""}`}>{qr ? <QrCode text={qr} size={268} /> : null}</div>
        <div class="t-caption c-muted" style="margin-top:12px;text-align:center">
          {t("onboarding.link.scanHint")}
        </div>
        <div ref={(el) => {
          backRef.current = el;
        }} style="margin-top:18px;display:flex;justify-content:center">
          <G kind="row">
            <TextButton label={p.backLabel} initial={!needsNew} onSelect={p.onBack} />
          </G>
        </div>
      </div>
      {p.globe && <LanguageGlobe class="globe-corner" onOpen={() => setChoosingLanguage(true)} />}
      {p.globe && choosingLanguage && <LanguagePanel onClose={() => setChoosingLanguage(false)} />}
    </BrandGlow>
  );
}

function Step(p: { n: number; children: ComponentChildren }) {
  return (
    <div class="link-step">
      <div class="step-n t-label">{p.n}</div>
      <div class="t-body-large c-fg one-line" style="margin-left:14px">
        {p.children}
      </div>
    </div>
  );
}
