import { useState, type FormEvent } from "react";
import { useSearchParams } from "react-router";
import { Trans, useTranslation } from "react-i18next";
import { Fingerprint } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useMySso, useRequestSsoLink, useUnlinkSso } from "@/api/hooks";
import { useAuth } from "@/auth";
import { useToast } from "@/components/Toast";
import { Button, Card, CardTitle, Field, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";
import { appShell } from "@/lib/shell";
import { atPublicAddress, ssoNotice, ssoStartUrl } from "@/lib/sso";
import { codeKind } from "@/lib/twoStep";

/**
 * Signing in with the single sign-on provider: linking this account to an
 * identity there, or unlinking it. Both take the account's password, and
 * linking happens in a browser at Pley's public address, where the provider
 * sends people back. The desktop and Android apps' windows don't go to the
 * provider's pages, so they send people to a browser for it.
 */
export function SingleSignOnCard() {
  const { t } = useTranslation(["pages", "common"]);
  const user = useAuth();
  const toast = useToast();
  const [params] = useSearchParams();
  const sso = useMySso(!user.managed);
  const link = useRequestSsoLink();
  const unlink = useUnlinkSso();
  // Changing how the account signs in takes its password: a session alone isn't enough.
  const [asking, setAsking] = useState<"link" | "unlink" | null>(null);
  const [password, setPassword] = useState("");
  // Signing in through the provider skips two-step sign-in: linking takes its code.
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const info = sso.data;
  if (user.managed || !info?.enabled) return null;
  const needsCode = asking === "link" && user.two_step;
  const name = info.name ?? t("sso.theProvider");
  const notice = ssoNotice(params.get("sso"), name);
  const publicUrl = info.public_url ?? "";
  const here = atPublicAddress(publicUrl);
  const inApp = appShell() !== null;
  const busy = link.isPending || unlink.isPending;

  const cancel = () => {
    setAsking(null);
    setPassword("");
    setCode("");
    setError(null);
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (asking === "link") {
      link.mutate(needsCode ? { password, code: code.trim() } : { password }, {
        // Off to the provider; it sends the browser back here.
        onSuccess: () => window.location.assign(ssoStartUrl(info.start ?? "", "/account")),
        onError: (err) => setError(errorMessage(err)),
      });
    } else if (asking === "unlink") {
      unlink.mutate(password, {
        onSuccess: () => {
          toast.success(t("sso.account.unlinked", { provider: name }));
          cancel();
        },
        onError: (err) => setError(errorMessage(err)),
      });
    }
  };

  return (
    <Card>
      <CardTitle icon={<Fingerprint />}>{t("sso.account.title")}</CardTitle>
      {notice ? (
        <p role={notice.tone === "error" ? "alert" : "status"} className={cn("mb-3 text-sm", notice.tone === "success" ? "text-success" : "text-danger")}>
          {notice.text}
        </p>
      ) : null}
      <p className="text-sm text-muted">
        {info.linked ? (
          info.email ? (
            <Trans
              t={t}
              i18nKey="sso.account.signsInWithAs"
              values={{ provider: name, email: info.email }}
              components={{ email: <span className="text-fg" /> }}
            />
          ) : (
            t("sso.account.signsInWith", { provider: name })
          )
        ) : (
          t("sso.account.offer", { provider: name })
        )}
      </p>
      {!info.has_password ? (
        <p className="mt-2 text-sm text-subtle">{t("sso.account.noPassword", { provider: name })}</p>
      ) : inApp ? (
        <p className="mt-2 text-sm text-subtle">{t("sso.account.inBrowser", { provider: name })}</p>
      ) : !here ? (
        <p className="mt-2 text-sm text-subtle">
          <Trans
            t={t}
            i18nKey="sso.account.elsewhere"
            values={{ provider: name, address: publicUrl.replace(/^https?:\/\//, "") }}
            components={{ address: <a href={`${publicUrl}/account`} className="text-accent hover:underline" /> }}
          />
        </p>
      ) : asking ? (
        <form onSubmit={onSubmit} className="mt-4 space-y-3">
          <Field label={t("fields.yourPassword")} htmlFor="sso-password" hint={t("sso.account.passwordHint")}>
            <TextInput
              id="sso-password"
              type="password"
              autoComplete="current-password"
              autoFocus
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
            />
          </Field>
          {needsCode ? (
            <Field label={t("sso.account.code")} htmlFor="sso-code">
              <TextInput
                id="sso-code"
                autoComplete="one-time-code"
                autoCapitalize="off"
                spellCheck={false}
                value={code}
                onChange={(e) => setCode(e.target.value)}
                required
              />
            </Field>
          ) : null}
          {error ? <p className="text-sm text-danger">{error}</p> : null}
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={cancel} disabled={busy}>
              {t("common:actions.cancel")}
            </Button>
            <Button
              type="submit"
              variant={asking === "unlink" ? "danger" : "primary"}
              loading={busy}
              disabled={!password || (needsCode && !codeKind(code))}
            >
              {asking === "unlink" ? t("sso.account.unlinkFrom", { provider: name }) : t("sso.account.continueTo", { provider: name })}
            </Button>
          </div>
        </form>
      ) : (
        <div className="mt-4 flex justify-end">
          {info.linked ? (
            <Button variant="outline" onClick={() => setAsking("unlink")}>
              {t("sso.account.unlink")}
            </Button>
          ) : (
            <Button variant="primary" onClick={() => setAsking("link")}>
              {t("sso.account.link", { provider: name })}
            </Button>
          )}
        </div>
      )}
    </Card>
  );
}
