import { t } from "@/i18n";

/** Shortest password the server accepts. */
export const MIN_PASSWORD_LENGTH = 8;

/**
 * What's wrong with a new password that can be told without asking the
 * server (which also refuses common passwords), or `null`.
 */
export function passwordProblem(password: string, username = ""): string | null {
  if ([...password].length < MIN_PASSWORD_LENGTH) return t("passwords.tooShort", { min: MIN_PASSWORD_LENGTH });
  if (username.trim() && password.toLowerCase() === username.trim().toLowerCase()) return t("passwords.sameAsUsername");
  return null;
}
