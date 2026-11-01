import { isApiError } from "@/api/client";
import type { ServerStatus, TwoStepSetup, User } from "@/api/types";
import { saveFile } from "./download";

/** The server's answer to a sign-in that needs its second step, or got a wrong code. */
export function isTwoStepError(err: unknown): boolean {
  return isApiError(err, 401) && err.code === "two_step";
}

/**
 * The secret to add to an authenticator app, when the server answered a
 * sign-in (or a switch, or an invitation) by asking to set up two-step
 * sign-in first, which it requires; `null` for any other answer.
 */
export function twoStepSetupOf(err: unknown): TwoStepSetup | null {
  if (!isApiError(err, 401) || err.code !== "two_step_setup") return null;
  const setup = (err.body as { setup?: TwoStepSetup } | undefined)?.setup;
  return setup && typeof setup.secret === "string" && typeof setup.uri === "string" && setup.qr ? setup : null;
}

/** Whether `user` has to set up two-step sign-in, which the server requires, at their next sign-in. */
export function mustSetUpTwoStep(user: User, server: ServerStatus | undefined): boolean {
  return !!server?.two_step_required && user.has_password && !user.managed && !user.two_step;
}

/** What a typed code is: one from the authenticator app (6 digits), a recovery code, or neither yet. */
export function codeKind(input: string): "app" | "recovery" | null {
  const compact = input.replace(/[\s-]/g, "");
  if (/^\d{6}$/.test(compact)) return "app";
  if (/^[0-9a-z]{16}$/i.test(compact)) return "recovery";
  return null;
}

/** An authenticator secret in groups of four, as people type it: `"JBSW Y3DP EHPK 3PXP"`. */
export function groupSecret(secret: string): string {
  return secret.replace(/(.{4})(?=.)/g, "$1 ");
}

/** The recovery codes as a text file. */
export function recoveryCodesText(codes: string[], username: string, server?: string): string {
  return [
    `Pley recovery codes for ${username}${server ? ` on ${server}` : ""}`,
    "Each code signs in once, instead of a code from your authenticator app.",
    "",
    ...codes,
    "",
  ].join("\n");
}

/** Saves `text` as a file called `name`. */
export function downloadText(name: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: "text/plain;charset=utf-8" }));
  saveFile(url, name);
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
