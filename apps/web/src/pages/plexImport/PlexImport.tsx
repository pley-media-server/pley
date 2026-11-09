import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ExternalLink } from "lucide-react";
import { errorMessage, isApiError } from "@/api/client";
import {
  useChoosePlexServer,
  usePlexImport,
  usePlexPlan,
  usePlexServers,
  usePlexSignedIn,
  usePlexToken,
  useStartPlexImport,
  useStartPlexSignIn,
  plexImportKeys,
  type ImportBring,
  type ImportChoices,
  type ImportPlan,
} from "@/api/plexImport";
import { useSharedLibraries } from "@/api/hooks";
import { Button, Field, PageSpinner, TextInput } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { useOnChange } from "@/lib/hooks";
import { choicesFor, firstChoices, libraryProblems, type LibraryChoices } from "./choices";
import { LibrariesStep } from "./LibrariesStep";
import { BRING_ALL, BringStep } from "./BringStep";
import { PeopleStep, type PeopleChoices } from "./PeopleStep";
import { ImportProgressView } from "./ImportProgressView";

type Stage = "signIn" | "server" | "libraries" | "people" | "bring" | "importing";

/** The steps counted in "Step 2 of 4"; importing isn't one. */
const STEPS: Stage[] = ["signIn", "server", "libraries", "people", "bring"];

/** How often Plex's sign-in is asked about while its page is open. */
const SIGN_IN_POLL_MS = 2_000;

export function ErrorNote({ children }: { children: ReactNode }) {
  return (
    <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
      {children}
    </p>
  );
}

/** A step: its place, title and words, what it asks, and the buttons that leave it. */
export function StepFrame({
  stage,
  title,
  description,
  children,
  actions,
}: {
  stage?: Stage;
  title: string;
  description?: ReactNode;
  children: ReactNode;
  actions?: ReactNode;
}) {
  const { t } = useTranslation("settings");
  const index = stage ? STEPS.indexOf(stage) : -1;
  const titleId = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  // A new step replaces the one whose button was pressed: the focus goes to
  // its title, read out, rather than to the page.
  useEffect(() => {
    const focused = document.activeElement;
    if (!focused || focused === document.body) heading.current?.focus();
  }, []);
  return (
    <section className="space-y-5" aria-labelledby={titleId}>
      <div>
        {index >= 0 ? (
          <p className="text-xs font-medium uppercase tracking-wide text-subtle">
            {t("plexImport.stepOf", { step: index + 1, total: STEPS.length })}
          </p>
        ) : null}
        <h2 id={titleId} ref={heading} tabIndex={-1} className="mt-0.5 text-base font-semibold outline-none">
          {title}
        </h2>
        {description ? <p className="mt-1 text-sm text-muted">{description}</p> : null}
      </div>
      {children}
      {actions ? <div className="flex flex-wrap items-center justify-end gap-2 pt-1">{actions}</div> : null}
    </section>
  );
}

/**
 * Moving from Plex, a step at a time: signing in with Plex, its server, how
 * its libraries pair with Pley's, the people who come as profiles, then the
 * import's progress and report. The first-run page and Settings › Libraries
 * both show it; `onLeave` is their way out before the import starts,
 * `onFinish` after its report.
 */
export function PlexImport({
  onLeave,
  onFinish,
  leaveLabel,
  onBusy,
}: {
  onLeave: () => void;
  onFinish: () => void;
  /** What the way out before the import is called ("Cancel", "Start fresh instead"). */
  leaveLabel: string;
  /** Told when a request that mustn't be cut short starts or ends. */
  onBusy?: (busy: boolean) => void;
}) {
  const progress = usePlexImport();
  const signedIn = usePlexSignedIn(false);
  const [stage, setStage] = useState<Stage | null>(null);
  const plan = usePlexPlan(stage === "libraries" || stage === "people" || stage === "bring");
  const libraries = useSharedLibraries();
  const [libraryChoices, setLibraryChoices] = useState<LibraryChoices>({});
  const [people, setPeople] = useState<PeopleChoices>({});
  const [bring, setBring] = useState<ImportBring>(BRING_ALL);
  const start = useStartPlexImport();
  const qc = useQueryClient();
  // The server whose plan the choices were made on, and whether the only
  // server was connected to on its own: Back to the server step shows it
  // instead of connecting again, and choosing it again keeps the choices.
  const chosenServer = useRef<string | null>(null);
  const [autoChosen, setAutoChosen] = useState(false);

  // Choices start from the plan each time it's fetched (a server chosen again).
  useOnChange(plan.data ?? null, (data) => {
    if (!data) return;
    setLibraryChoices(firstChoices(data));
    setPeople(Object.fromEntries(data.users.filter((u) => !u.admin).map((u) => [u.id, { bring: true, pin: "" }])));
  });

  useEffect(() => onBusy?.(start.isPending), [start.isPending, onBusy]);

  if (progress.isPending || (signedIn.isPending && !stage)) return <PageSpinner />;

  // The first step, set once: an import already running is shown, wherever
  // it was started from, through to its report.
  const shown: Stage = stage ?? (progress.data?.state === "running" ? "importing" : signedIn.data === true ? "server" : "signIn");
  if (stage === null) setStage(shown);

  const chosen = (server: string) => {
    if (chosenServer.current !== server) qc.removeQueries({ queryKey: plexImportKeys.plan });
    chosenServer.current = server;
    setStage("libraries");
  };

  const leave = (
    <Button variant="ghost" onClick={onLeave}>
      {leaveLabel}
    </Button>
  );

  switch (shown) {
    case "signIn":
      return <SignInStep leave={leave} onSignedIn={() => setStage("server")} />;
    case "server":
      return (
        <ServerStep
          leave={leave}
          auto={!autoChosen}
          onAuto={() => setAutoChosen(true)}
          onBack={() => setStage("signIn")}
          onChosen={chosen}
        />
      );
    case "libraries":
      return (
        <PlanStep plan={plan} onSignedOut={() => setStage("signIn")}>
          {(data) => (
            <LibrariesStep
              plan={data}
              libraries={libraries.data ?? []}
              choices={libraryChoices}
              onChange={setLibraryChoices}
              onBack={() => setStage("server")}
              onNext={() => setStage("people")}
            />
          )}
        </PlanStep>
      );
    case "people":
      return (
        <PlanStep plan={plan} onSignedOut={() => setStage("signIn")}>
          {(data) => (
            <PeopleStep plan={data} choices={people} onChange={setPeople} onBack={() => setStage("libraries")} onNext={() => setStage("bring")} />
          )}
        </PlanStep>
      );
    case "bring":
      return (
        <PlanStep plan={plan} onSignedOut={() => setStage("signIn")}>
          {(data) => (
            <BringStep
              bring={bring}
              onChange={setBring}
              onBack={() => setStage("people")}
              starting={start.isPending}
              error={start.error ? errorMessage(start.error) : null}
              onStart={() => {
                // Checked again here: the libraries step may have been left by Back.
                if (Object.keys(libraryProblems(data, libraryChoices)).length > 0) {
                  setStage("libraries");
                  return;
                }
                const choices: ImportChoices = {
                  ...choicesFor(data, libraryChoices),
                  users: data.users
                    .filter((u) => !u.admin && people[u.id]?.bring)
                    .map((u) => {
                      const pin = people[u.id]?.pin.trim();
                      return pin ? { id: u.id, pin } : { id: u.id };
                    }),
                  bring,
                };
                start.mutate(choices, { onSuccess: () => setStage("importing") });
              }}
            />
          )}
        </PlanStep>
      );
    case "importing":
      return (
        <ImportProgressView
          onFinish={onFinish}
          onAgain={() => {
            setAutoChosen(false);
            setStage(signedIn.data === true ? "server" : "signIn");
          }}
        />
      );
  }
}

/** Waits for the plan, which reads every library of Plex's: a moment on a big server. */
function PlanStep({
  plan,
  onSignedOut,
  children,
}: {
  plan: ReturnType<typeof usePlexPlan>;
  onSignedOut: () => void;
  children: (plan: ImportPlan) => ReactNode;
}) {
  const { t } = useTranslation(["settings", "common"]);
  if (plan.data) return <>{children(plan.data)}</>;
  if (plan.isError) {
    const signedOut = isApiError(plan.error) && (plan.error.code === "plex_signed_out" || plan.error.code === "plex_no_server");
    return (
      <StepFrame
        title={t("plexImport.plan.couldntRead")}
        actions={
          signedOut ? (
            <Button variant="primary" onClick={onSignedOut}>
              {t("plexImport.signIn.again")}
            </Button>
          ) : (
            <Button variant="primary" onClick={() => void plan.refetch()}>
              {t("common:actions.retry")}
            </Button>
          )
        }
      >
        <ErrorNote>{errorMessage(plan.error)}</ErrorNote>
      </StepFrame>
    );
  }
  return (
    <div className="flex flex-col items-center gap-3 py-10 text-sm text-muted">
      <PageSpinner />
      <p>{t("plexImport.plan.reading")}</p>
    </div>
  );
}

function SignInStep({ leave, onSignedIn }: { leave: ReactNode; onSignedIn: () => void }) {
  const { t } = useTranslation(["settings", "common"]);
  const startSignIn = useStartPlexSignIn();
  const tokenSignIn = usePlexToken();
  const [waiting, setWaiting] = useState(false);
  const [authUrl, setAuthUrl] = useState<string | null>(null);
  const [withToken, setWithToken] = useState(false);
  const [token, setToken] = useState("");
  const popup = useRef<Window | null>(null);
  const status = usePlexSignedIn(waiting ? SIGN_IN_POLL_MS : false);
  const signedIn = waiting && status.data === true;
  const expired = waiting && isApiError(status.error) && status.error.code === "plex_pin_expired";

  useEffect(() => {
    if (!signedIn) return;
    // Plex's page stays open on "you can close this window": closed for
    // the person, where the browser still lets this page.
    try {
      popup.current?.close();
    } catch {
      // Out of this page's reach.
    }
    onSignedIn();
  }, [signedIn, onSignedIn]);

  const signIn = () => {
    // Opened in the click, before Plex's address is known, or the browser
    // would block it; it goes there once the server has made the PIN.
    const win = window.open("", "pley-plex-sign-in", "popup,width=720,height=760");
    popup.current = win;
    startSignIn.mutate(undefined, {
      onSuccess: ({ auth_url }) => {
        setAuthUrl(auth_url);
        if (win && !win.closed) win.location.href = auth_url;
        setWaiting(true);
        void status.refetch();
      },
      onError: () => win?.close(),
    });
  };

  const submitToken = (e: FormEvent) => {
    e.preventDefault();
    if (!token.trim()) return;
    tokenSignIn.mutate(token.trim(), { onSuccess: onSignedIn });
  };

  const error = startSignIn.error ?? (expired ? null : waiting ? status.error : null);

  return (
    <StepFrame
      stage="signIn"
      title={t("plexImport.signIn.title")}
      description={t("plexImport.signIn.description")}
      actions={leave}
    >
      <div className="space-y-3">
        {/* Never held while waiting: Plex's window may have been closed. */}
        <Button variant="primary" size="lg" className="w-full" onClick={signIn} loading={startSignIn.isPending}>
          {t("plexImport.signIn.button")}
        </Button>
        {waiting && !expired && authUrl ? (
          <p role="status" className="text-sm text-muted">
            {t("plexImport.signIn.waiting")} {t("plexImport.signIn.noWindow")}{" "}
            <a href={authUrl} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-accent hover:underline">
              {t("plexImport.signIn.openPage")}
              <ExternalLink className="size-3.5" />
            </a>
          </p>
        ) : null}
        {expired ? <ErrorNote>{t("plexImport.signIn.expired")}</ErrorNote> : null}
        {error ? <ErrorNote>{errorMessage(error)}</ErrorNote> : null}
      </div>

      <div className="border-t border-border pt-4">
        {withToken ? (
          <form onSubmit={submitToken} className="space-y-3">
            <Field label={t("plexImport.signIn.token")} htmlFor="plex-token" hint={t("plexImport.signIn.tokenHint")} error={tokenSignIn.error ? errorMessage(tokenSignIn.error) : null}>
              <TextInput
                id="plex-token"
                type="password"
                autoComplete="off"
                spellCheck={false}
                value={token}
                onChange={(e) => setToken(e.target.value)}
                className="font-mono"
              />
            </Field>
            <div className="flex justify-end">
              <Button type="submit" loading={tokenSignIn.isPending} disabled={!token.trim()}>
                {t("plexImport.signIn.useToken")}
              </Button>
            </div>
          </form>
        ) : (
          <button type="button" className="text-sm text-muted hover:text-fg hover:underline" onClick={() => setWithToken(true)}>
            {t("plexImport.signIn.withToken")}
          </button>
        )}
      </div>
    </StepFrame>
  );
}

function ServerStep({
  leave,
  auto,
  onAuto,
  onBack,
  onChosen,
}: {
  leave: ReactNode;
  /** Connect to the only server without asking: the first time only. */
  auto: boolean;
  onAuto: () => void;
  onBack: () => void;
  /** With the server's id, or the address typed. */
  onChosen: (server: string) => void;
}) {
  const { t } = useTranslation(["settings", "common"]);
  const servers = usePlexServers(true);
  const choose = useChoosePlexServer();
  const [picked, setPicked] = useState<string | null>(null);
  const [address, setAddress] = useState("");
  // Read once: telling the import the only server was connected to (so
  // Back shows the list) mustn't turn this visit's connecting off.
  const [connectAlone] = useState(auto);
  const tried = useRef(false);
  // Only a server the account owns can be imported; one shared with it is
  // listed to say whose account to sign in with.
  const list = (servers.data ?? []).filter((s) => s.owned);
  const shared = (servers.data ?? []).filter((s) => !s.owned);

  // One server: nothing to choose, so it's connected to straight away,
  // with the list shown only if that fails.
  const only = connectAlone && list.length === 1 ? list[0] : undefined;
  useEffect(() => {
    if (!only || tried.current) return;
    tried.current = true;
    onAuto();
    choose.mutate({ id: only.id }, { onSuccess: () => onChosen(only.id) });
  }, [only, choose, onAuto, onChosen]);

  if (servers.isPending || (only && !choose.isError)) {
    return (
      <div className="flex flex-col items-center gap-3 py-10 text-sm text-muted">
        <PageSpinner />
        <p>{only ? t("plexImport.server.connecting", { name: only.name }) : t("plexImport.server.finding")}</p>
      </div>
    );
  }

  if (servers.isError) {
    const signedOut = isApiError(servers.error) && servers.error.code === "plex_signed_out";
    return (
      <StepFrame stage="server" title={t("plexImport.server.title")} actions={
        <>
          {leave}
          <Button variant="primary" onClick={signedOut ? onBack : () => void servers.refetch()}>
            {signedOut ? t("plexImport.signIn.again") : t("common:actions.retry")}
          </Button>
        </>
      }>
        <ErrorNote>{errorMessage(servers.error)}</ErrorNote>
      </StepFrame>
    );
  }

  if (list.length === 0) {
    const names = new Intl.ListFormat(currentLocale(), { type: "conjunction" }).format(shared.map((s) => s.name));
    return (
      <StepFrame
        stage="server"
        title={t("plexImport.server.title")}
        description={shared.length > 0 ? t("plexImport.server.sharedOnly", { servers: names }) : t("plexImport.server.none")}
        actions={
          <>
            {leave}
            <Button variant="primary" onClick={onBack}>
              {t("plexImport.server.otherAccount")}
            </Button>
          </>
        }
      >
        {null}
      </StepFrame>
    );
  }

  const selected = picked ?? (address.trim() ? null : (list[0]?.id ?? null));
  const submit = (e: FormEvent) => {
    e.preventDefault();
    const typed = address.trim();
    if (typed) choose.mutate({ address: typed }, { onSuccess: () => onChosen(`address:${typed}`) });
    else if (selected) choose.mutate({ id: selected }, { onSuccess: () => onChosen(selected) });
  };

  return (
    <form onSubmit={submit}>
      <StepFrame
        stage="server"
        title={t("plexImport.server.title")}
        description={t("plexImport.server.description")}
        actions={
          <>
            <Button variant="ghost" onClick={onBack}>
              {t("common:actions.back")}
            </Button>
            <Button type="submit" variant="primary" loading={choose.isPending} disabled={!selected && !address.trim()}>
              {t("common:actions.continue")}
            </Button>
          </>
        }
      >
        {list.length > 0 ? (
          <div role="radiogroup" aria-label={t("plexImport.server.title")} className="space-y-2">
            {list.map((s) => (
              <label
                key={s.id}
                className={cn(
                  "flex cursor-pointer items-center gap-3 rounded-lg border px-3 py-2.5 text-sm transition-colors",
                  selected === s.id && !address.trim() ? "border-accent bg-accent/10" : "border-border bg-surface-2 hover:border-border-strong",
                )}
              >
                <input
                  type="radio"
                  name="plex-server"
                  className="accent-[var(--color-accent)]"
                  checked={selected === s.id && !address.trim()}
                  onChange={() => {
                    setPicked(s.id);
                    setAddress("");
                  }}
                />
                <span className="min-w-0 flex-1 truncate font-medium">{s.name}</span>
                {s.version ? <span className="text-xs text-subtle">{t("plexImport.server.version", { version: s.version })}</span> : null}
              </label>
            ))}
          </div>
        ) : null}
        <Field label={t("plexImport.server.address")} htmlFor="plex-address" hint={t("plexImport.server.addressHint")}>
          <TextInput
            id="plex-address"
            inputMode="url"
            spellCheck={false}
            placeholder="http://192.168.1.20:32400"
            value={address}
            onChange={(e) => setAddress(e.target.value)}
          />
        </Field>
        {choose.error ? <ErrorNote>{errorMessage(choose.error)}</ErrorNote> : null}
      </StepFrame>
    </form>
  );
}
