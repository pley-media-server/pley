import { createContext, useContext } from "react";
import type { CastTarget } from "@/api/cast";
import type { CastAction, Id, ItemSummary } from "@/api/types";

/** What to play on the device picked. */
export interface CastRequest {
  item: ItemSummary;
  /** The version to play; left out, the device picks as it would. */
  fileId?: Id | null;
  /** From a player: where it is now (ms). Left out, the device starts where the profile left off. */
  position?: () => number;
  /** The device was sent it: a player steps aside. */
  onSent?: () => void;
}

/** A title sent to a device that hasn't said yet that it plays it. */
export interface CastPending {
  deviceId: string;
  itemId: Id;
  /** When it was sent (`Date.now()`). */
  sentAt: number;
}

export interface CastApi {
  /** The devices this one may cast to (not itself). */
  targets: CastTarget[];
  /** Opens the "Play on" picker for `request`. */
  choose: (request: CastRequest) => void;
  /** Opens the remote of a device. */
  showRemote: (deviceId: string) => void;
  /** Pauses, resumes, seeks or stops a device's player. */
  steer: (deviceId: string, action: CastAction, positionMs?: number) => void;
  /** The device last cast to from here. */
  last: string | null;
  /** The picker or a remote is open. */
  open: boolean;
}

export const CastContext = createContext<CastApi | null>(null);

/** Casting, where the signed-in pages offer it (`null` elsewhere). */
export function useCast(): CastApi | null {
  return useContext(CastContext);
}
