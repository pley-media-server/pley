import { useState, type FormEvent, type ReactNode } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Check, Copy, Plug, Plus } from "lucide-react";
import { errorMessage } from "@/api/client";
import {
  allowedFromLabel,
  type CreatedCompatibilityToken,
  FROM_LOCAL,
  MAX_ALLOWED_FROM,
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

/** The entries typed for "only from", separated by commas, spaces or new lines, each once. */
function parseAllowedFrom(text: string): string[] {
  return [...new Set(text.split(/[\s,]+/).filter(Boolean))];
}

/** A checkbox row, as the network settings' choices. */
function Choice({
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

/**
 * A name, what the token may do and where from. Administrators may allow
 * more than watching; the server refuses that to anyone else anyway.
 */
function NewTokenForm({ onMade, onCancel }: { onMade: (made: CreatedCompatibilityToken) => void; onCancel: () => void }) {
  const { t } = useTranslation(["pages", "common"]);
  const me = useAuth();
  const create = useCreateCompatibilityToken();
  const [name, setName] = useState("");
  const [scopes, setScopes] = useState<string[]>(["play"]);
  const [local, setLocal] = useState(false);
  const [others, setOthers] = useState("");
  const [error, setError] = useState<string | null>(null);
  const choices = TOKEN_SCOPES.filter((s) => !s.admin || me.is_admin);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!name.trim()) return setError(t("tokens.needName"));
    const allowed = [...new Set([...(local ? [FROM_LOCAL] : []), ...parseAllowedFrom(others)])];
    if (allowed.length > MAX_ALLOWED_FROM) return setError(t("tokens.tooMany", { max: MAX_ALLOWED_FROM }));
    create.mutate(
      {
        name: name.trim(),
        scope: TOKEN_SCOPES.filter((s) => scopes.includes(s.scope)).map((s) => s.scope).join(" "),
        allowed_from: allowed,
      },
      { onSuccess: onMade, onError: (err) => setError(errorMessage(err)) },
    );
  };

  return (
    <form onSubmit={submit} className="mb-4 space-y-4 rounded-md border border-border p-4">
      <Field label={t("tokens.name")} htmlFor="token-name" hint={t("tokens.nameHint")}>
        <TextInput id="token-name" maxLength={MAX_NAME} autoComplete="off" value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      <fieldset>
        <legend className="mb-1.5 text-sm font-medium">{t("tokens.mayDo")}</legend>
        {choices.map((s) => (
          <Choice
            key={s.scope}
            id={`token-scope-${s.scope}`}
            checked={scopes.includes(s.scope)}
            // Every token may watch.
            disabled={s.scope === "play"}
            onChange={() => setScopes((on) => (on.includes(s.scope) ? on.filter((x) => x !== s.scope) : [...on, s.scope]))}
          >
            <span className="font-medium">{s.label}</span>
            <span className="block text-xs text-subtle">{s.hint}</span>
          </Choice>
        ))}
      </fieldset>
      <Field label={t("tokens.onlyFrom")} htmlFor="token-allowed-from" hint={t("tokens.onlyFromHint")}>
        <Choice id="token-local" checked={local} onChange={() => setLocal((on) => !on)}>
          {t("tokens.local")}
        </Choice>
        <textarea
          id="token-allowed-from"
          rows={2}
          autoComplete="off"
          spellCheck={false}
          value={others}
          placeholder="10.0.0.0/24, 192.168.1.20"
          onChange={(e) => setOthers(e.target.value)}
          className="w-full resize-y rounded-md border border-border bg-surface-2 px-3 py-2 font-mono text-sm text-fg placeholder:text-subtle transition-colors focus:border-accent focus:outline-none"
        />
      </Field>
      {error ? <p className="text-sm text-danger">{error}</p> : null}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>
          {t("common:actions.cancel")}
        </Button>
        <Button type="submit" variant="primary" loading={create.isPending}>
          {t("tokens.make")}
        </Button>
      </div>
    </form>
  );
}

/** A new token, shown once, to copy into the app. */
function NewToken({ made, onDone }: { made: CreatedCompatibilityToken; onDone: () => void }) {
  const { t } = useTranslation(["pages", "common"]);
  const toast = useToast();
  return (
    <div className="mb-4 space-y-3 rounded-md border border-accent/40 bg-accent/5 p-4">
      <p className="text-sm">
        <Trans t={t} i18nKey="tokens.copyNow" values={{ name: made.name }} components={{ bold: <span className="font-medium" /> }} />
      </p>
      <div className="flex items-center gap-2">
        <TextInput readOnly aria-label={t("tokens.new")} value={made.token} className="font-mono" onFocus={(e) => e.target.select()} />
        <Button
          icon={<Copy className="size-4" />}
          onClick={() =>
            void copyText(made.token).then((ok) => (ok ? toast.success(t("tokens.copied")) : toast.error(t("tokens.notCopied"))))
          }
        >
          {t("common:actions.copy")}
        </Button>
      </div>
      <div className="flex justify-end">
        <Button variant="ghost" onClick={onDone}>
          {t("common:actions.done")}
        </Button>
      </div>
    </div>
  );
}

/**
 * The apps signed in through Pley's Jellyfin-compatible API, and tokens made
 * for other apps, tools and scripts: each works on the compatible APIs only,
 * for what it may do and from where it may be used.
 */
export function CompatibilityTokensCard() {
  const { t } = useTranslation(["pages", "common"]);
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
          <Plug className="size-4 text-accent" /> {t("tokens.title")}
        </h2>
        {making || made ? null : (
          <Button size="sm" variant="ghost" icon={<Plus className="size-4" />} onClick={() => setMaking(true)}>
            {t("tokens.new")}
          </Button>
        )}
      </div>
      <p className="mb-4 text-sm text-muted">{t("tokens.subtitle")}</p>
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
        <p className="text-sm text-subtle">{t("common:states.loading")}</p>
      ) : tokens.isError ? (
        <p className="text-sm text-danger">{errorMessage(tokens.error)}</p>
      ) : list.length === 0 ? (
        <p className="text-sm text-subtle">{t("tokens.none")}</p>
      ) : (
        <ul className="divide-y divide-white/[0.05] rounded-md border border-border">
          {list.map((token) => {
            const allows = [tokenScopesLabel(token.scopes), allowedFromLabel(token.allowed_from)].filter(Boolean).join(" · ");
            return (
              <li key={token.id} className="flex items-center gap-3 px-3 py-2.5">
                <Plug className="size-5 shrink-0 text-subtle" />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium">{token.name}</div>
                  <div className="truncate text-xs text-muted" title={allows}>
                    {allows}
                  </div>
                  <div className="truncate text-xs text-subtle">
                    {[
                      token.device,
                      token.ip,
                      token.last_used_at ? t("tokens.lastUsed", { when: formatAgo(token.last_used_at) }) : t("tokens.notUsed"),
                      t("tokens.made", { when: formatAgo(token.created_at) }),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={t("tokens.revokeNamed", { name: token.name })}
                  loading={revoke.isPending && revoke.variables === token.id}
                  onClick={() =>
                    revoke.mutate(token.id, {
                      onSuccess: () => toast.success(t("tokens.revoked", { name: token.name })),
                      onError: (err) => toast.error(errorMessage(err)),
                    })
                  }
                >
                  {t("tokens.revoke")}
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}
