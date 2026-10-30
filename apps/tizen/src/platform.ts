import { localStore } from "pley-tv/auth/tokens";
import type { DeviceFacts, Platform, Probe } from "pley-tv/platform";
import { standardKey, type Key } from "pley-tv/ui/keys";
import { tierOf, tizenCaps } from "./caps";
import { AvplayEngine } from "./engine";
import { readFacts, type TizenFacts } from "./facts";

/** pley for Samsung TV's built-in OAuth client (apps/server/src/oauth/builtin/tizen.json). */
export const CLIENT_ID = "https://pley-media-server.github.io/pley/oauth/tizen.json";

/** The remote's keys the app asks Tizen for (left to the TV unless registered). Exit stays the TV's. */
export const REGISTERED = [
  "MediaPlayPause",
  "MediaPlay",
  "MediaPause",
  "MediaStop",
  "MediaFastForward",
  "MediaRewind",
  "MediaTrackNext",
  "MediaTrackPrevious",
  "ColorF0Red",
  "ColorF1Green",
  "ColorF2Yellow",
  "ColorF3Blue",
  "Info",
  "Caption",
  "Tools",
  "ChannelUp",
  "ChannelDown",
  "ChannelList",
  "Guide",
  "0",
  "1",
  "2",
  "3",
  "4",
  "5",
  "6",
  "7",
  "8",
  "9",
];

/** Samsung's key names, by meaning. */
const BY_NAME: Record<string, Key> = {
  MediaPlayPause: "playPause",
  MediaPlay: "play",
  MediaPause: "pause",
  MediaStop: "stop",
  MediaFastForward: "fastForward",
  MediaRewind: "rewind",
  MediaTrackNext: "next",
  MediaTrackPrevious: "previous",
  ColorF0Red: "red",
  ColorF1Green: "green",
  ColorF2Yellow: "yellow",
  ColorF3Blue: "blue",
  Info: "info",
  Caption: "captions",
  Tools: "menu",
  ChannelUp: "channelUp",
  ChannelDown: "channelDown",
  ChannelList: "channelList",
  Guide: "guide",
};

/** The codes Samsung documents for its remote, used until the TV tells its own. */
const DEFAULT_CODES: Record<number, Key> = {
  10009: "back",
  10252: "playPause",
  415: "play",
  19: "pause",
  413: "stop",
  417: "fastForward",
  412: "rewind",
  10233: "next",
  10232: "previous",
  403: "red",
  404: "green",
  405: "yellow",
  406: "blue",
  457: "info",
  10221: "captions",
  10135: "menu",
  427: "channelUp",
  428: "channelDown",
  10073: "channelList",
  458: "guide",
};

/** Key codes to meanings: Samsung's names from the TV (codes differ between models), else the documented codes. */
export function keyMap(supported: TizenKey[] | null): Record<number, Key> {
  const map: Record<number, Key> = { ...DEFAULT_CODES };
  for (const k of supported ?? []) {
    const meaning = BY_NAME[k.name];
    if (meaning) map[k.code] = meaning;
  }
  return map;
}

/**
 * pley for Samsung TVs: AVPlay for video, Samsung's Product API for what
 * the TV is, the remote's keys by Tizen's names.
 */
export function tizenPlatform(): Platform {
  let facts: TizenFacts | null = null;
  const getFacts = () => (facts ??= readFacts());
  let keys: Record<number, Key> = DEFAULT_CODES;
  return {
    name: "tizen",
    clientId: CLIENT_ID,
    version: __PLEY_VERSION__,
    appName: "Pley for Samsung TV",
    storage: localStore,
    mapKey(e: KeyboardEvent): Key | null {
      return keys[e.keyCode] ?? standardKey(e);
    },
    registerKeys() {
      try {
        const supported = tizen!.tvinputdevice.getSupportedKeys();
        const names = supported.map((k) => k.name);
        const wanted = REGISTERED.filter((n) => names.indexOf(n) >= 0);
        if (tizen!.tvinputdevice.registerKeyBatch) tizen!.tvinputdevice.registerKeyBatch(wanted);
        else for (const n of wanted) tizen!.tvinputdevice.registerKey(n);
        keys = keyMap(supported);
      } catch {
        // Not on a TV.
      }
    },
    createEngine: (host) => new AvplayEngine(host, getFacts().uhd),
    async probe(overrides): Promise<Probe> {
      const r = tizenCaps(getFacts(), overrides);
      return { caps: r.caps, lines: r.lines };
    },
    device(): DeviceFacts {
      const f = getFacts();
      return { maker: "Samsung", model: f.model, os: `Tizen ${f.tizen.toFixed(1)}`, firmware: f.firmware, year: f.year, chromium: f.chromium };
    },
    tier: () => tierOf(getFacts()),
    async localAddress(): Promise<string | null> {
      try {
        return webapis?.network?.getIp() ?? null;
      } catch {
        return null;
      }
    },
    keepAwake(on) {
      try {
        const common = webapis!.appcommon!;
        common.setScreenSaver(on ? common.AppCommonScreenSaverState.SCREEN_SAVER_OFF : common.AppCommonScreenSaverState.SCREEN_SAVER_ON);
      } catch {
        // Not on a TV.
      }
    },
    asksBeforeExit: true,
    exit() {
      try {
        tizen!.application.getCurrentApplication().exit();
      } catch {
        window.close();
      }
    },
    onVisibility(listener) {
      document.addEventListener("visibilitychange", () => listener(!document.hidden));
    },
    clock24() {
      try {
        return !/ap/i.test(tizen!.time!.getTimeFormat());
      } catch {
        return false;
      }
    },
    screenReader(listener) {
      // Voice Guide (Settings › General › Accessibility), from Samsung's TV info API.
      const info = typeof webapis !== "undefined" ? webapis.tvinfo : undefined;
      if (!info) return;
      const key = info.TvInfoMenuKey.VOICE_GUIDE_KEY;
      const read = () => {
        try {
          const v = info.getMenuValue(key);
          listener(v === info.TvInfoMenuValue.ON || String(v) === "true");
        } catch {
          // This TV doesn't say.
        }
      };
      read();
      try {
        info.addCaptionChangeListener(key, read);
      } catch {
        // Read once.
      }
    },
  };
}
