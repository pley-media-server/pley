import { useSyncExternalStore } from "react";

/**
 * The sessions this tab's players play now, so the remote leaves them out:
 * a tab controls the user's other devices, not itself. Each player says so
 * as a session starts, and lets go as it ends.
 */
let here: ReadonlySet<string> = new Set();
const listeners = new Set<() => void>();

function changed(next: ReadonlySet<string>) {
  here = next;
  for (const listener of listeners) listener();
}

/** `sessionId` plays in this tab until the returned function is called. */
export function playingHere(sessionId: string): () => void {
  changed(new Set([...here, sessionId]));
  return () => {
    if (!here.has(sessionId)) return;
    const next = new Set(here);
    next.delete(sessionId);
    changed(next);
  };
}

/** Whether `sessionId` plays in this tab: how the players' tests see that a player said so. */
export function isPlayingHere(sessionId: string): boolean {
  return here.has(sessionId);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The sessions this tab plays, kept up to date. */
export function useSessionsHere(): ReadonlySet<string> {
  return useSyncExternalStore(subscribe, () => here);
}
