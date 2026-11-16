import { t } from "@/i18n";

/** What a single sign-on that came back says (`?sso=<code>` on the sign-in and account pages). */
export interface SsoNotice {
  tone: "success" | "error";
  text: string;
}

/**
 * The notice for `code`, naming the `provider`; `null` without a code. The
 * server keeps the details in its log: these say what to do next.
 */
export function ssoNotice(code: string | null, provider: string): SsoNotice | null {
  if (!code) return null;
  const error = (text: string): SsoNotice => ({ tone: "error", text });
  switch (code) {
    case "linked":
      return { tone: "success", text: t("pages:sso.notice.linked", { provider }) };
    case "off":
      return error(t("pages:sso.notice.off"));
    case "expired":
      return error(t("pages:sso.notice.expired"));
    case "denied":
      return error(t("pages:sso.notice.denied", { provider }));
    case "provider":
      return error(t("pages:sso.notice.provider", { provider }));
    case "invalid":
      return error(t("pages:sso.notice.invalid", { provider }));
    case "unknown":
      return error(t("pages:sso.notice.unknown", { provider }));
    case "disabled":
      return error(t("pages:sso.notice.disabled"));
    case "group":
      return error(t("pages:sso.notice.group", { provider }));
    case "signin":
      return error(t("pages:sso.notice.signin"));
    case "taken":
      return error(t("pages:sso.notice.taken", { provider }));
    case "two_step":
      return error(t("pages:sso.notice.twoStep"));
    case "busy":
      return error(t("pages:sso.notice.busy"));
    default:
      return error(t("pages:sso.notice.error"));
  }
}

/** Where to send the browser to sign in with the provider, coming back to `next`. */
export function ssoStartUrl(start: string, next: string): string {
  return `${start}?next=${encodeURIComponent(next)}`;
}

/** Whether this page is at Pley's public address, where linking happens. */
export function atPublicAddress(publicUrl: string | undefined, origin: string = window.location.origin): boolean {
  if (!publicUrl) return true;
  try {
    return new URL(publicUrl).origin === origin;
  } catch {
    return true;
  }
}
