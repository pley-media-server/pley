import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Network } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useLinkTailscale, useMyTailscale, useUnlinkTailscale } from "@/api/hooks";
import type { MyTailscale } from "@/api/types";
import { useAuth } from "@/auth";
import { useToast } from "@/components/Toast";
import { Button, Card, CardTitle, Field, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";
import { formatDay } from "@/lib/format";
import { codeKind } from "@/lib/twoStep";

/**
 * Signing in with Tailscale: the Tailscale user linked to the account, which
 * signs in without the password from devices on the tailnet. Shown while the
 * server has it on, or the account is still linked.
 */
export function TailscaleCard() {
  const { t } = useTranslation(["pages", "common"]);
  const user = useAuth();
  const usable = !user.managed && user.has_password;
  const mine = useMyTailscale(usable);
  const [linking, setLinking] = useState(false);
  if (!usable || !mine.data || (!mine.data.on && !mine.data.linked)) return null;

  return (
    <Card>
      <CardTitle icon={<Network />}>{t("tailscale.title")}</CardTitle>
      {linking && mine.data.here ? (
        <LinkForm login={mine.data.here.login} onDone={() => setLinking(false)} />
      ) : (
        <Status mine={mine.data} onLink={() => setLinking(true)} />
      )}
    </Card>
  );
}

function Status({ mine, onLink }: { mine: MyTailscale; onLink: () => void }) {
  const { t } = useTranslation(["pages", "common"]);
  const user = useAuth();
  const unlink = useUnlinkTailscale();
  const toast = useToast();
  // Administrators sign in with Tailscale only with two-step sign-in on.
  const needsTwoStep = user.is_admin && !user.two_step;
  // A Tailscale user here that isn't linked yet, or was linked to this
  // account before another; not one linked to another account.
  const canLink = mine.on && mine.here && mine.here.username !== user.username && !mine.here.elsewhere;
  const linkedHere = mine.linked && mine.here?.username === user.username;

  return (
    <>
      <p className="text-sm text-muted">
        {!mine.on
          ? t("tailscale.off")
          : mine.linked
            ? t("tailscale.linkedTo", { login: mine.linked.login, date: formatDay(mine.linked.linked_at) })
            : t("tailscale.intro")}
      </p>
      {mine.on ? (
        <p className="mt-2 text-sm text-subtle">
          {mine.here
            ? linkedHere
              ? t("tailscale.thisDevice")
              : mine.here.elsewhere
                ? t("tailscale.hereElsewhere", { login: mine.here.login })
                : t("tailscale.here", { login: mine.here.login })
            : mine.server_name
              ? t("tailscale.notHereNamed", { name: mine.server_name })
              : t("tailscale.notHere")}
        </p>
      ) : null}
      {mine.problem ? <p className="mt-2 text-sm text-danger">{t("tailscale.problem", { problem: mine.problem })}</p> : null}
      {canLink && needsTwoStep ? <p className="mt-2 text-sm text-danger">{t("tailscale.adminNeedsTwoStep")}</p> : null}
      {unlink.error ? <p className="mt-2 text-sm text-danger">{errorMessage(unlink.error)}</p> : null}
      <div className="mt-4 flex flex-wrap justify-end gap-2 empty:hidden">
        {mine.linked ? (
          <Button
            variant="ghost"
            loading={unlink.isPending}
            title={t("tailscale.unlinkHint")}
            onClick={() => unlink.mutate(undefined, { onSuccess: () => toast.success(t("tailscale.unlinkedToast")) })}
          >
            {t("tailscale.unlink")}
          </Button>
        ) : null}
        {canLink && mine.here ? (
          <Button variant={mine.linked ? "outline" : "primary"} disabled={needsTwoStep} onClick={onLink}>
            {t("tailscale.link", { login: mine.here.login })}
          </Button>
        ) : null}
      </div>
    </>
  );
}

/** Linking adds a way in: it takes the password, and a code with two-step sign-in. */
function LinkForm({ login, onDone }: { login: string; onDone: () => void }) {
  const { t } = useTranslation(["pages", "common"]);
  const user = useAuth();
  const link = useLinkTailscale();
  const toast = useToast();
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const withCode = user.two_step;

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    link.mutate(withCode ? { password, code: code.trim() } : { password }, {
      onSuccess: () => {
        toast.success(t("tailscale.linkedToast", { login }));
        onDone();
      },
    });
  };

  return (
    <form onSubmit={onSubmit} className="space-y-3">
      <p className="text-sm text-muted">{t("tailscale.linkHint", { login })}</p>
      <div className={cn("grid gap-3", withCode && "sm:grid-cols-2")}>
        <Field label={t("fields.yourPassword")} htmlFor="tailscale-password">
          <TextInput
            id="tailscale-password"
            type="password"
            autoComplete="current-password"
            autoFocus
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
          />
        </Field>
        {withCode ? (
          <Field label={t("fields.code")} htmlFor="tailscale-code">
            <TextInput
              id="tailscale-code"
              autoComplete="one-time-code"
              autoCapitalize="off"
              spellCheck={false}
              value={code}
              onChange={(e) => setCode(e.target.value)}
              required
            />
          </Field>
        ) : null}
      </div>
      {link.error ? <p className="text-sm text-danger">{errorMessage(link.error)}</p> : null}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onDone} disabled={link.isPending}>
          {t("common:actions.cancel")}
        </Button>
        <Button type="submit" variant="primary" loading={link.isPending} disabled={!password || (withCode && !codeKind(code))}>
          {t("tailscale.linkButton")}
        </Button>
      </div>
    </form>
  );
}
