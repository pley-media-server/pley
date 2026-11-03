// Casting (docs/API.md, "Casting to another device"): the profile's devices
// that may be cast to, what to play on one and how to steer it.

import { queryOptions, useQuery, type QueryClient } from "@tanstack/react-query";
import { api } from "./client";
import type { CastAction, CastDevice, CastState, Id } from "./types";

/** What a device plays, and when this page heard it (`Date.now()`): a playing position moves on from then. */
export interface HeardCastState extends CastState {
  heard_at: number;
}

export interface CastTarget extends Omit<CastDevice, "state"> {
  state: HeardCastState | null;
}

export const castKeys = {
  all: ["cast"] as const,
  devices: ["cast", "devices"] as const,
};

function heard(state: CastState | null, at: number): HeardCastState | null {
  return state ? { ...state, heard_at: at } : null;
}

export const castQueries = {
  // The event stream keeps it up to date: `cast_devices_changed` asks again, `cast_state` changes it in place.
  devices: () =>
    queryOptions({
      queryKey: castKeys.devices,
      queryFn: async ({ signal }): Promise<CastTarget[]> => {
        const devices = await api.get<CastDevice[]>("/api/cast/devices", { signal });
        const at = Date.now();
        return devices.map((d) => ({ ...d, state: heard(d.state, at) }));
      },
      staleTime: Infinity,
    }),
};

/**
 * The profile's devices that may be cast to, but `self` (this device,
 * when it is one of them).
 */
export function useCastTargets(self: string | null): CastTarget[] {
  const devices = useQuery(castQueries.devices()).data;
  return (devices ?? []).filter((d) => d.id !== self);
}

/** A device said what it plays (`cast_state`): the list says so at once. */
export function patchCastState(qc: QueryClient, deviceId: string, state: CastState | null) {
  const at = Date.now();
  let known = false;
  qc.setQueryData<CastTarget[]>(castKeys.devices, (old) =>
    old?.map((d) => {
      if (d.id !== deviceId) return d;
      known = true;
      return { ...d, state: heard(state, at) };
    }),
  );
  // A device not listed yet: its arrival is on the way, but ask anyway.
  if (!known) void qc.invalidateQueries({ queryKey: castKeys.devices });
}

/** Where a device is now (ms): what it said, moved on by the time since, while it plays; never past the end. */
export function castPosition(state: HeardCastState, now: number = Date.now()): number {
  const moved = state.state === "playing" ? state.age_ms + Math.max(0, now - state.heard_at) : 0;
  const at = state.position_ms + moved;
  return state.duration_ms ? Math.min(at, state.duration_ms) : at;
}

const devicePath = (id: string) => `/api/cast/devices/${encodeURIComponent(id)}`;

/** Device `id` plays `item_id`; without `position_ms`, where the profile left off. */
export function castPlay(id: string, body: { item_id: Id; file_id?: Id | null; position_ms?: number }): Promise<void> {
  return api.post<void>(`${devicePath(id)}/play`, body);
}

/** Device `id`'s player pauses, resumes, seeks (to `positionMs`) or stops. */
export function castControl(id: string, action: CastAction, positionMs?: number): Promise<void> {
  return api.post<void>(`${devicePath(id)}/control`, positionMs === undefined ? { action } : { action, position_ms: Math.round(positionMs) });
}

export interface CastReport {
  /** `null`: it plays nothing. */
  item_id: Id | null;
  position_ms?: number;
  duration_ms?: number;
  state?: CastState["state"];
}

/** This device (`id`) says what it plays. */
export function putCastState(id: string, report: CastReport, signal?: AbortSignal): Promise<void> {
  return api.put<void>(`${devicePath(id)}/state`, report, { signal });
}
