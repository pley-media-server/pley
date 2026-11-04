import { api, isApiError } from "@/api/client";
import type { Id, PlaybackInfo, PlaybackRequest, ProgressRequest } from "@/api/types";

/**
 * Fire-and-forget progress report. `keepalive` lets it survive page unloads.
 * `onStopped` hears a 410: the session was stopped on the server (for a
 * direct play, the only request that can tell).
 */
export function reportProgress(
  itemId: Id,
  body: ProgressRequest,
  keepalive = false,
  onStopped?: () => void,
): Promise<void> {
  return api
    .post<void>(`/api/items/${itemId}/progress`, body, { keepalive, noAuthRedirect: keepalive })
    .catch((err: unknown) => {
      if (isApiError(err, 410)) onStopped?.();
    });
}

/** Why playback was ended on the server (`DELETE /api/sessions/{id}`). */
export interface StoppedBy {
  message: string | null;
  /** `null`: not known (the stream only answered 410). */
  byAdmin: boolean | null;
}

/**
 * Whether the server ended this stream (it answers `410` once a session is
 * stopped), with its message. `null`: it didn't, or it can't be asked.
 */
export async function stoppedStream(url: string): Promise<StoppedBy | null> {
  try {
    const res = await fetch(url, { credentials: "same-origin", headers: { Range: "bytes=0-0" } });
    if (res.status !== 410) {
      void res.body?.cancel().catch(() => undefined);
      return null;
    }
    const body = (await res.json().catch(() => null)) as { message?: unknown } | null;
    return { message: typeof body?.message === "string" && body.message ? body.message : null, byAdmin: null };
  } catch {
    return null;
  }
}

/**
 * Remember the chosen tracks for this show (or movie); `subtitle_index: null` = off.
 * `audio_index` left out keeps the audio chosen before (subtitles alone were picked).
 */
export function saveStreamChoice(
  itemId: Id,
  body: { file_id: Id | null; audio_index?: number | null; subtitle_index: number | null },
): Promise<void> {
  return api.put<void>(`/api/items/${itemId}/streams`, body).catch(() => undefined);
}

export function stopSession(sessionId: string, keepalive = false): Promise<void> {
  return api
    .del<void>(`/api/playback/${encodeURIComponent(sessionId)}`, { keepalive, noAuthRedirect: keepalive })
    .catch(() => undefined);
}

/**
 * Report the final position of a session and release it (its transcode slot)
 * in parallel. Resolves once both requests are done; never rejects. `tracks`:
 * what a video played, which its reports say.
 */
export function endSession(
  itemId: Id,
  session: { session_id: string; duration_ms: number },
  positionMs: number,
  keepalive = false,
  tracks: Pick<ProgressRequest, "audio_index" | "subtitle_index"> = {},
): Promise<void> {
  return Promise.all([
    reportProgress(
      itemId,
      {
        position_ms: Math.max(0, Math.round(positionMs)),
        duration_ms: session.duration_ms || undefined,
        state: "stopped",
        session_id: session.session_id,
        ...tracks,
      },
      keepalive,
    ),
    stopSession(session.session_id, keepalive),
  ]).then(() => undefined);
}

/**
 * Not abortable on purpose: the server may already have created the session
 * (and its transcode) when the request is cancelled. Callers that no longer
 * need the result must `stopSession` it.
 */
export function startPlayback(req: Omit<PlaybackRequest, "client_id">): Promise<PlaybackInfo> {
  return api.post<PlaybackInfo>("/api/playback", { ...req, client_id: clientId() });
}

const CLIENT_KEY = "pley.clientId";
let currentClientId: string | null = null;

function randomId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes); // works outside secure contexts, unlike randomUUID
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Identifies this browser tab to the server, which ends a client's previous
 * playback session whenever it starts a new one. The id survives reloads but
 * is not inherited by duplicated tabs: it only sits in sessionStorage while
 * the page is being unloaded.
 */
export function clientId(): string {
  if (currentClientId) return currentClientId;
  let stored: string | null = null;
  try {
    stored = window.sessionStorage.getItem(CLIENT_KEY);
    window.sessionStorage.removeItem(CLIENT_KEY);
  } catch {
    // storage unavailable
  }
  const id = stored || randomId();
  currentClientId = id;
  window.addEventListener("pagehide", () => {
    try {
      window.sessionStorage.setItem(CLIENT_KEY, id);
    } catch {
      // storage unavailable
    }
  });
  window.addEventListener("pageshow", (e) => {
    // Back from the bfcache: take the id out of storage again.
    if (!e.persisted) return;
    try {
      window.sessionStorage.removeItem(CLIENT_KEY);
    } catch {
      // storage unavailable
    }
  });
  return id;
}
