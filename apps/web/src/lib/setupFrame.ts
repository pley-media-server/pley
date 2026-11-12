import { createContext, useContext } from "react";

/**
 * Setup's card (`SetupLayout`), to the steps inside it: they say how far
 * setup is, and what waits or fails draws itself to fit in the card.
 */
export interface SetupFrame {
  setProgress: (value: number) => void;
}

export const SetupFrameContext = createContext<SetupFrame | null>(null);

/** Setup's card, when this is inside it. */
export function useSetupFrame(): SetupFrame | null {
  return useContext(SetupFrameContext);
}

/**
 * How far setup is on each of its steps: the account, the server, how to
 * start, then Start fresh's libraries or the Plex import, whose own steps
 * fill the last quarter.
 */
export const SETUP_PROGRESS = { account: 0.25, server: 0.5, start: 0.75, libraries: 1 } as const;

/** How far setup is on the Plex import's step `index` of `total`. */
export function plexProgress(index: number, total: number): number {
  return SETUP_PROGRESS.start + ((1 - SETUP_PROGRESS.start) * (index + 1)) / total;
}
