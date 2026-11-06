import { useState, type FormEvent } from "react";
import { useSearchParams } from "react-router";
import { useMutation } from "@tanstack/react-query";
import { errorMessage } from "@/api/client";
import {
  decideDevice,
  decideQuickConnect,
  quickConnectConsent,
  verifyCode,
  verifyQuickConnect,
  type ConsentRequest,
} from "@/api/oauth";
import { Button, Field, TextInput } from "@/components/ui";
import { useDocumentTitle } from "@/lib/hooks";
import { hasStrayLetter, isQuickConnectCode, isWholeCode, showCode, typedCode } from "@/lib/userCode";
import { AppConsent, appName } from "./AppConsent";
import { AuthLayout } from "./AuthLayout";

/** The request a code leads to: a TV's, or a Jellyfin app's Quick Connect request. */
function lookUp(code: string): Promise<ConsentRequest> {
  return isQuickConnectCode(code) ? verifyQuickConnect(code).then(quickConnectConsent) : verifyCode(code);
}

/**
 * Signs in a TV (OAuth device authorization, RFC 8628): the TV shows a code
 * and this page's address; here, on a phone or a computer, the person types
 * the code and allows the app. The TV hears the answer when it next asks the
 * server. A code in the address (from the TV's QR code) is only filled in:
 * the person compares it with the TV first (RFC 8628 §5.4).
 *
 * A Jellyfin app's Quick Connect code (6 digits) is typed here too, and the
 * app signs in to the Jellyfin-compatible API the same way.
 */
export function LinkPage() {
  useDocumentTitle("Sign in on your TV");
  const [params] = useSearchParams();
  const linked = typedCode(params.get("code") ?? "");
  /** The code's letters (without the dash) or digits. */
  const [code, setCode] = useState(linked);
  const verify = useMutation({ mutationFn: lookUp });
  /** The TV's request, with the code that found it. */
  const [found, setFound] = useState<{ request: ConsentRequest; code: string } | null>(null);
  const [answer, setAnswer] = useState<"allowed" | "denied" | null>(null);
  const whole = (typed: string) => isWholeCode(typed) || isQuickConnectCode(typed);
  /** A whole code from the address, as it came: to compare with the TV. */
  const checking = whole(linked) && code === linked;
  const jellyfin = found?.request.redirect.kind === "quick-connect";

  if (found && answer === "allowed") {
    return (
      <AuthLayout title={jellyfin ? "The Jellyfin app is signing in" : `${appName(found.request.client)} on your TV is signing in`}>
        <p className="text-sm text-muted">You can close this page.</p>
      </AuthLayout>
    );
  }
  if (found && answer === "denied") {
    return (
      <AuthLayout title="Not allowed">
        <p className="text-sm text-muted">{jellyfin ? "The app won't be signed in." : "Your TV will say so."}</p>
      </AuthLayout>
    );
  }
  if (found) {
    const { request } = found;
    const decide = jellyfin ? decideQuickConnect : decideDevice;
    return (
      <AppConsent
        request={request}
        returnTo={`/link?code=${found.code}`}
        onAnswer={(allow) => decide(request.id, allow).then(() => setAnswer(allow ? "allowed" : "denied"))}
      />
    );
  }

  const type = (value: string) => {
    setCode(typedCode(value));
    // The server's answer was about the code before.
    if (verify.isError) verify.reset();
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!whole(code) || verify.isPending) return;
    verify.mutate(code, { onSuccess: (request, sent) => setFound({ request, code: sent }) });
  };

  // A Quick Connect code's digits are no letters at all.
  const stray = /^[A-Z]/.test(code) && hasStrayLetter(code);

  return (
    <AuthLayout
      title="Sign in on your TV"
      subtitle={
        checking
          ? "Check that your TV or app shows this code, then continue."
          : "Enter the code your TV shows, or a Jellyfin app's Quick Connect code."
      }
    >
      <form onSubmit={submit} className="space-y-4">
        <Field
          label="Code"
          htmlFor="code"
          error={stray ? "Codes don't use A, E, I, O, U or Y: check the code on your TV." : null}
        >
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
          disabled={!whole(code)}
        >
          Continue
        </Button>
      </form>
    </AuthLayout>
  );
}
