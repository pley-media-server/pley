import { useSyncExternalStore } from "react";

/**
 * Whether the server is out of reach (restarting, off, or away from this
 * network): what the pages show is what they loaded before. The server's
 * event stream (api/events.ts) says so when it has been down a few seconds
 * and the server doesn't answer, and stops saying so the moment it is back.
 */
let lost = false;
const listeners = new Set<() => void>();

export const connectionLost = (): boolean => lost;

export function setConnectionLost(value: boolean): void {
  if (lost === value) return;
  lost = value;
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useConnectionLost(): boolean {
  return useSyncExternalStore(subscribe, () => lost, () => false);
}

/**
 * Whether the server's event stream is open, so changes arrive as they
 * happen: while it isn't, what would hear of them asks again now and then.
 */
let live = false;

export const eventsLive = (): boolean => live;

export function setEventsLive(value: boolean): void {
  live = value;
}

/**
 * Whether the server answers at all (its health check), within `timeoutMs`:
 * a stream it refused (a sign-in that ended) is no lost connection.
 */
export async function serverAnswers(timeoutMs = 5_000): Promise<boolean> {
  try {
    const response = await fetch("/api/health", { cache: "no-store", signal: AbortSignal.timeout(timeoutMs) });
    return response.ok;
  } catch {
    return false;
  }
}
