import type { Id } from "@/api/types";

/**
 * Short ids for page URLs: `/item/kx7bq2m` rather than `/item/1234`. The
 * API keeps its numbers; links carry them scrambled (a fixed 32-bit
 * permutation) and spelled with consonants and digits, so neighboring
 * numbers don't look related and no ids spell words. They hide nothing: the
 * server checks access as it always does. Plain numbers (older links) still
 * work.
 */

/** No vowels (no words), no l (looks like 1). */
const LETTERS = "bcdfghjkmnpqrstvwxz";
const CHARS = `${LETTERS}23456789`;
const LENGTH = 7;
const KEYS = [0x6a09, 0xbb67, 0x3c6e, 0xa54f];

function round(half: number, key: number): number {
  return (Math.imul((half ^ key) >>> 0, 0x45d9f3b) >>> 11) & 0xffff;
}

/** A Feistel network over 32 bits: a permutation, undone by running the rounds backwards. */
function scramble(value: number, decode: boolean): number {
  let left = value >>> 16;
  let right = value & 0xffff;
  const keys = decode ? [...KEYS].reverse() : KEYS;
  for (const key of keys) {
    if (decode) [left, right] = [right ^ round(left, key), left];
    else [left, right] = [right, left ^ round(right, key)];
  }
  return ((left << 16) | right) >>> 0;
}

/** `1234` → `"kx7bq2m"` (7 characters, starting with a letter). */
export function encodeId(id: Id): string {
  if (!Number.isInteger(id) || id < 0 || id > 0xffffffff) return String(id);
  let value = scramble(id, false);
  let out = LETTERS.charAt(value % LETTERS.length);
  value = Math.floor(value / LETTERS.length);
  for (let i = 1; i < LENGTH; i++) {
    out += CHARS.charAt(value % CHARS.length);
    value = Math.floor(value / CHARS.length);
  }
  return out;
}

/** `decodeId` for route params: `NaN` when the URL holds no id. */
export function idParam(param: string | undefined): Id {
  return decodeId(param) ?? Number.NaN;
}

/** The id in a URL: a short id, or a plain number from an older link; `null` for anything else. */
export function decodeId(param: string | null | undefined): Id | null {
  if (!param) return null;
  if (/^[1-9]\d{0,15}$/.test(param)) return Number(param);
  const text = param.toLowerCase();
  if (text.length !== LENGTH || !LETTERS.includes(text.charAt(0))) return null;
  let value = 0;
  for (let i = LENGTH - 1; i >= 1; i--) {
    const digit = CHARS.indexOf(text.charAt(i));
    if (digit < 0) return null;
    value = value * CHARS.length + digit;
  }
  value = value * LETTERS.length + LETTERS.indexOf(text.charAt(0));
  if (value > 0xffffffff) return null;
  return scramble(value, true);
}
