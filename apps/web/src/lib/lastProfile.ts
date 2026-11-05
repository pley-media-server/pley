import type { User } from "@/api/types";
import { getStored, writeStorage } from "./hooks";

/** The profile this browser was last used as, so signing in again here goes straight in as it. */
const LAST_PROFILE_KEY = "pley.lastProfile";

/** The profile this browser was last used as; `null` before any. */
export function lastProfile(): number | null {
  const id = getStored<unknown>(LAST_PROFILE_KEY, null);
  return typeof id === "number" && Number.isSafeInteger(id) && id > 0 ? id : null;
}

/** This browser is used as profile `id` now. */
export function rememberProfile(id: number) {
  if (lastProfile() !== id) writeStorage(LAST_PROFILE_KEY, id);
}

/**
 * Whether signing in as `user` asks "Who's watching?" first: in a household
 * of several profiles, unless this browser was last used as `user` (`last`)
 * and it has no PIN. Then it goes straight in, as the apps do as they start.
 */
export function asksWhoIsWatching(user: User, household: number, last: number | null): boolean {
  if (household <= 1) return false;
  return last !== user.id || user.has_pin;
}
