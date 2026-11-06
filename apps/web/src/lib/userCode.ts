/**
 * The codes TVs show to sign in (OAuth device authorization, RFC 8628
 * §6.1): 8 letters out of 20 consonants, so no words and nothing that looks
 * alike, shown as "BCDF-GHJK". Case, spaces and the dash don't matter.
 *
 * Jellyfin's apps show Quick Connect codes instead: 6 digits, as Jellyfin
 * makes them. The first letter or digit typed says which kind a code is.
 */

/** The letters codes are made of. */
export const CODE_LETTERS = "BCDFGHJKLMNPQRSTVWXZ";

/** How many letters a code has. */
export const CODE_LENGTH = 8;

/** The letters of a code as typed, in capitals: spaces, dashes and other signs are left out, and so is a 9th letter. */
export function codeLetters(input: string): string {
  return input
    .replace(/[^a-z]/gi, "")
    .toUpperCase()
    .slice(0, CODE_LENGTH);
}

/** How many digits a Quick Connect code has. */
export const QUICK_CONNECT_LENGTH = 6;

/** A code as typed: a Quick Connect code's digits when it starts with one, else a TV's letters ({@link codeLetters}). */
export function typedCode(input: string): string {
  const first = /[a-z0-9]/i.exec(input)?.[0] ?? "";
  return /\d/.test(first) ? input.replace(/\D/g, "").slice(0, QUICK_CONNECT_LENGTH) : codeLetters(input);
}

/** Whether `code` is a whole Quick Connect code. */
export function isQuickConnectCode(code: string): boolean {
  return code.length === QUICK_CONNECT_LENGTH && /^\d+$/.test(code);
}

/**
 * The Quick Connect code in the address of Jellyfin's web app that apps open
 * to approve theirs (`/web/#/quickconnect?code=123456`, also as a QR code):
 * the server sends such an address to `/link`, and browsers keep what
 * follows `#`. A whole code or "".
 */
export function quickConnectCodeInHash(hash: string): string {
  const query = /^#\/quickconnect\?(.*)$/i.exec(hash)?.[1];
  const code = query === undefined ? "" : (new URLSearchParams(query).get("code") ?? "");
  return isQuickConnectCode(code) ? code : "";
}

/** Letters as a TV shows them, as far as they go: "BCDF", "BCDF-G", "BCDF-GHJK". Digits stay as they are. */
export function showCode(letters: string): string {
  return letters.length > 4 && !/^\d/.test(letters) ? `${letters.slice(0, 4)}-${letters.slice(4)}` : letters;
}

/** Whether there's a letter no code has: A, E, I, O, U or Y. */
export function hasStrayLetter(letters: string): boolean {
  return [...letters].some((letter) => !CODE_LETTERS.includes(letter));
}

/** Whether `letters` are a whole code. */
export function isWholeCode(letters: string): boolean {
  return letters.length === CODE_LENGTH && !hasStrayLetter(letters);
}
