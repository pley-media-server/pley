import { useState, type FormEvent, type ReactNode } from "react";
import { Check, Copy, Plug, Plus } from "lucide-react";
import { errorMessage } from "@/api/client";
import {
  type CreatedCompatibilityToken,
  TOKEN_SCOPES,
  tokenScopesLabel,
  useCompatibilityTokens,
  useCreateCompatibilityToken,
  useRevokeCompatibilityToken,
} from "@/api/compatibilityTokens";
import { useAuth } from "@/auth";
import { useToast } from "@/components/Toast";
import { Button, Card, Field, TextInput } from "@/components/ui";
import { copyText } from "@/lib/clipboard";
import { cn } from "@/lib/cn";
import { formatAgo } from "@/lib/format";

/** The longest name the server takes. */
const MAX_NAME = 80;

/** A checkbox row, as the network settings' choices. */
function ScopeChoice({
  id,
  checked,
  disabled,
  onChange,
  children,
}: {
  id: string;
  checked: boolean;
  disabled?: boolean;
  onChange: () => void;
  children: ReactNode;
}) {
  return (
    <label
      htmlFor={id}
      className={cn("flex items-start gap-3 rounded px-2 py-2 text-sm", disabled ? "opacity-60" : "cursor-pointer hover:bg-white/5")}
    >
      <input id={id} type="checkbox" checked={checked} disabled={disabled} onChange={onChange} className="peer sr-only" />
      <span
        className={cn(
          "mt-0.5 flex size-4 shrink-0 items-center justify-center rounded border transition-colors peer-focus-visible:ring-2 peer-focus-visible:ring-accent",
          checked ? "border-accent bg-accent text-accent-fg" : "border-border-strong",
        )}
      >
        {checked ? <Check className="size-3" strokeWidth={3} /> : null}
      </span>
      <span className="min-w-0 flex-1">{children}</span>
    </label>
  );
}

/** A name and what the token may do; administrators may allow more than watching. */
function NewTokenForm({ onMade, onCancel }: { onMade: (made: CreatedCompatibilityToken) => void; onCancel: () => void }) {
  const me = useAuth();
  const create = useCreateCompatibilityToken();
  const [name, setName] = useState("");
  const [scopes, setScopes] = useState<string[]>(["play"]);
  const [error, setError] = useState<string | null>(null);
  const choices = TOKEN_SCOPES.filter((s) => !s.admin || me.is_admin);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!name.trim()) return setError("Name the token after the app that will use it.");
    create.mutate(
      { name: name.trim(), scope: TOKEN_SCOPES.filter((s) => scopes.includes(s.scope)).map((s) => s.scope).join(" ") },
      { onSuccess: onMade, onError: (err) => setError(errorMessage(err)) },
    );
  };

  return (
    <form onSubmit={submit} className="mb-4 space-y-4 rounded-md border border-border p-4">
      <Field label="Name" htmlFor="token-name" hint="The app or tool that will use it, such as Tautulli.">
        <TextInput id="token-name" maxLength={MAX_NAME} autoComplete="off" value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      <fieldset>
        <legend className="mb-1.5 text-sm font-medium">What it may do</legend>
        {choices.map((s) => (
          <ScopeChoice
            key={s.scope}
            id={`token-scope-${s.scope}`}
            checked={scopes.includes(s.scope)}
            // Every token may watch.
            disabled={s.scope === "play"}
            onChange={() => setScopes((on) => (on.includes(s.scope) ? on.filter((x) => x !== s.scope) : [...on, s.scope]))}
          >
            <span className="font-medium">{s.label}</span>
            <span className="block text-xs text-subtle">{s.hint}</span>
          </ScopeChoice>
        ))}
      </fieldset>
      {error ? <p className="text-sm text-danger">{error}</p> : null}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" variant="primary" loading={create.isPending}>
          Make token
        </Button>
      </div>
    </form>
  );
}

/** A new token, shown once, to copy into the app. */
function NewToken({ made, onDone }: { made: CreatedCompatibilityToken; onDone: () => void }) {
  const toast = useToast();
  return (
    <div className="mb-4 space-y-3 rounded-md border border-accent/40 bg-accent/5 p-4">
      <p className="text-sm">
        The token for <span className="font-medium">{made.name}</span>. Copy it into the app now: Pley shows it only once.
      </p>
      <div className="flex items-center gap-2">
        <TextInput readOnly aria-label="New token" value={made.token} className="font-mono" onFocus={(e) => e.target.select()} />
        <Button
          icon={<Copy className="size-4" />}
          onClick={() => void copyText(made.token).then((ok) => (ok ? toast.success("Token copied") : toast.error("Couldn't copy the token")))}
        >
          Copy
        </Button>
      </div>
      <div className="flex justify-end">
        <Button variant="ghost" onClick={onDone}>
          Done
        </Button>
      </div>
    </div>
  );
}

/**
 * Tokens for other apps and tools that use Pley's compatibility API (Tautulli,
 * Kometa, Plezy, Infuse): each works there only, for what it may do.
 */
export function CompatibilityTokensCard() {
  const toast = useToast();
  const tokens = useCompatibilityTokens();
  const revoke = useRevokeCompatibilityToken();
  const [making, setMaking] = useState(false);
  const [made, setMade] = useState<CreatedCompatibilityToken | null>(null);
  const list = tokens.data ?? [];

  return (
    <Card>
      <div className="mb-2 flex items-center justify-between gap-3">
        <h2 className="flex items-center gap-2 text-base font-semibold">
          <Plug className="size-4 text-accent" /> Tokens for other apps
        </h2>
        {making || made ? null : (
          <Button size="sm" variant="ghost" icon={<Plus className="size-4" />} onClick={() => setMaking(true)}>
            New token
          </Button>
        )}
      </div>
      <p className="mb-4 text-sm text-muted">
        For apps and tools that use Pley's compatibility API, such as Tautulli, Kometa, Plezy or Infuse. A token works
        there only, for what you allow, and ends after 90 days unused or when you change your password.
      </p>
      {made ? <NewToken made={made} onDone={() => setMade(null)} /> : null}
      {making ? (
        <NewTokenForm
          onMade={(token) => {
            setMaking(false);
            setMade(token);
          }}
          onCancel={() => setMaking(false)}
        />
      ) : null}
      {tokens.isPending ? (
        <p className="text-sm text-subtle">Loading…</p>
      ) : tokens.isError ? (
        <p className="text-sm text-danger">{errorMessage(tokens.error)}</p>
      ) : list.length === 0 ? (
        <p className="text-sm text-subtle">No tokens yet. Make one for each app, so you can revoke one without the others.</p>
      ) : (
        <ul className="divide-y divide-white/[0.05] rounded-md border border-border">
          {list.map((t) => (
            <li key={t.id} className="flex items-center gap-3 px-3 py-2.5">
              <Plug className="size-5 shrink-0 text-subtle" />
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm font-medium">{t.name}</div>
                <div className="truncate text-xs text-muted">{tokenScopesLabel(t.scopes)}</div>
                <div className="truncate text-xs text-subtle">
                  {[t.device, t.ip, t.last_used_at ? `last used ${formatAgo(t.last_used_at)}` : "not used yet", `made ${formatAgo(t.created_at)}`]
                    .filter(Boolean)
                    .join(" · ")}
                </div>
              </div>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Revoke ${t.name}`}
                loading={revoke.isPending && revoke.variables === t.id}
                onClick={() =>
                  revoke.mutate(t.id, {
                    onSuccess: () => toast.success(`Revoked ${t.name}`),
                    onError: (err) => toast.error(errorMessage(err)),
                  })
                }
              >
                Revoke
              </Button>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
