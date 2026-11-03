import type { ClientCaps } from "./api/types";
import type { KeyValue } from "./auth/tokens";
import type { PlayerEngine } from "./player/engine";
import type { Key } from "./ui/keys";

/** What the person turned on in Settings for their receiver: formats sent as they are though the TV can't promise them. */
export interface CapsOverrides {
  truehd: boolean;
  dts: boolean;
}

/** The capabilities report, the lines Diagnostics shows, and what the report can't say. */
export interface Probe {
  caps: ClientCaps;
  lines: string[];
  /**
   * Containers the TV shows Dolby Vision from (LG before webOS 25: not
   * Matroska). A Dolby Vision file in another plays its base layer, so it
   * is asked for as a remux, which keeps it.
   */
  dvContainers?: string[];
}

/** A server found on the home network. */
export interface FoundServer {
  url: string;
  name: string;
  id: string | null;
  version: string;
}

/** What the TV reports of itself, for Diagnostics and the capabilities. */
export interface DeviceFacts {
  /** "Samsung". */
  maker: string;
  /** "QN65S95CAFXZA". */
  model: string;
  /** "Tizen 7.0". */
  os: string;
  firmware: string | null;
  /** The model year when the TV says it; else from its system version. */
  year: number | null;
  /** The web engine's Chromium major version. */
  chromium: number | null;
}

/**
 * What the TV app (apps/tizen) gives the screens: its keys, its player
 * (AVPlay), what the TV plays, and how it leaves.
 */
export interface Platform {
  name: "tizen";
  /** The app's OAuth client ID (built into the server). */
  clientId: string;
  version: string;
  /** "pley for Samsung TV". */
  appName: string;
  storage: KeyValue;
  /** The remote's keys, by meaning; `null` for keys the app leaves to the TV. */
  mapKey(e: KeyboardEvent): Key | null;
  /** Asks the TV for the keys it keeps for itself unless an app registers them (Tizen's media keys). */
  registerKeys?(): void;
  /** The TV's own player, its picture shown inside `host`. */
  createEngine(host: HTMLElement): PlayerEngine;
  /** The capabilities report (version 2) and the lines Diagnostics shows; probed after the first screen, again when Settings change it. */
  probe(overrides: CapsOverrides): Promise<Probe>;
  device(): DeviceFacts;
  /** "low" for TVs that struggle (little memory, old engines). */
  tier(): "low" | "mid" | "high";
  /** Servers on the home network that answer Pley's UDP call, when the TV lets an app send one. */
  discover?(onFound: (server: FoundServer) => void, stopped: () => boolean): Promise<void>;
  /** The TV's own IPv4 address on the home network, for "Search this network". */
  localAddress?(): Promise<string | null>;
  /** Keeps the screen on while a video plays. */
  keepAwake(on: boolean): void;
  /** Closes the app (Back on the rail). */
  exit(): void;
  /** Asks "Exit Pley?" first: Samsung's rule for Back on the first screen. */
  asksBeforeExit?: boolean;
  /** Tells the app when the TV hides or shows it (Home key, another app, standby). */
  onVisibility(listener: (visible: boolean) => void): void;
  /**
   * Whether the TV's screen reader (Samsung's Voice Guide) is on: told at
   * once, when the TV says, and on each change. It reads the element with
   * the browser's focus, which the app then moves along with its own.
   */
  screenReader?(listener: (on: boolean) => void): void;
  /** Whether the TV draws in 24-hour time. */
  clock24(): boolean;
}
