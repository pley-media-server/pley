import { useState, type FormEvent } from "react";
import { useSearchParams } from "react-router";
import { useTranslation } from "react-i18next";
import { useMutation } from "@tanstack/react-query";
import { errorMessage } from "@/api/client";
import { decideDevice, verifyCode, type ConsentRequest } from "@/api/oauth";
import { Button, Field, TextInput } from "@/components/ui";
import { useDocumentTitle } from "@/lib/hooks";
import { codeLetters, hasStrayLetter, isWholeCode, showCode } from "@/lib/userCode";
import { AppConsent, appName } from "./AppConsent";
import { AuthLayout } from "./AuthLayout";

/**
 * Signs in a TV (OAuth device authorization, RFC 8628): the TV shows a code
 * and this page's address; here, on a phone or a computer, the person types
 * the code and allows the app. The TV hears the answer when it next asks the
 * server. A code in the address (from the TV's QR code) is only filled in:
 * the person compares it with the TV first (RFC 8628 §5.4).
 */
export function LinkPage() {
  const { t } = useTranslation(["pages", "common"]);
  useDocumentTitle(t("link.title"));
  const [params] = useSearchParams();
  const linked = codeLetters(params.get("code") ?? "");
  /** The code's letters, without the dash. */
  const [code, setCode] = useState(linked);
  const verify = useMutation({ mutationFn: verifyCode });
  /** The TV's request, with the code that found it. */
  const [found, setFound] = useState<{ request: ConsentRequest; code: string } | null>(null);
  const [answer, setAnswer] = useState<"allowed" | "denied" | null>(null);
  /** A whole code from the address, as it came: to compare with the TV. */
  const checking = isWholeCode(linked) && code === linked;

  if (found && answer === "allowed") {
    return (
      <AuthLayout title={t("link.signingIn", { app: appName(found.request.client) })}>
        <p className="text-sm text-muted">{t("link.closePage")}</p>
      </AuthLayout>
    );
  }
  if (found && answer === "denied") {
    return (
      <AuthLayout title={t("link.notAllowed")}>
        <p className="text-sm text-muted">{t("link.tvWillSay")}</p>
      </AuthLayout>
    );
  }
  if (found) {
    const { request } = found;
    return (
      <AppConsent
        request={request}
        returnTo={`/link?code=${found.code}`}
        onAnswer={(allow) => decideDevice(request.id, allow).then(() => setAnswer(allow ? "allowed" : "denied"))}
      />
    );
  }

  const type = (value: string) => {
    setCode(codeLetters(value));
    // The server's answer was about the code before.
    if (verify.isError) verify.reset();
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!isWholeCode(code) || verify.isPending) return;
    verify.mutate(code, { onSuccess: (request, sent) => setFound({ request, code: sent }) });
  };

  return (
    <AuthLayout title={t("link.title")} subtitle={checking ? t("link.checkCode") : t("link.enterCode")}>
      <form onSubmit={submit} className="space-y-4">
        <Field label={t("fields.code")} htmlFor="code" error={hasStrayLetter(code) ? t("link.strayLetter") : null}>
          <TextInput
            id="code"
            // Typing comes first, unless there's a whole code to check.
            autoFocus={!checking}
            autoComplete="off"
            autoCapitalize="characters"
            spellCheck={false}
            inputMode="text"
            maxLength={9}
            placeholder="XXXX-XXXX"
            value={showCode(code)}
            onChange={(e) => type(e.target.value)}
            // Large, as the TV shows it: "!" wins over TextInput's own text-sm.
            className="indent-[0.3em] text-center font-mono text-2xl! font-semibold tracking-[0.3em]"
          />
        </Field>
        {verify.error ? (
          <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
            {errorMessage(verify.error)}
          </p>
        ) : null}
        <Button
          type="submit"
          variant="primary"
          size="lg"
          className="w-full"
          loading={verify.isPending}
          disabled={!isWholeCode(code)}
        >
          {t("common:actions.continue")}
        </Button>
      </form>
    </AuthLayout>
  );
}
