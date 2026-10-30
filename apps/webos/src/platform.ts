import type { DeviceFacts, Platform, Probe } from "pley-tv/platform";
import { localStore } from "pley-tv/auth/tokens";
import { standardKey, type Key } from "pley-tv/ui/keys";
import { browserFacts, readFacts, type WebosFacts } from "./facts";
import { browserCaps, tierOf, webosCaps } from "./caps";
import { WebosEngine } from "./engine";
import { VideoEngine } from "pley-tv/player/video-engine";
import { hasLuna, luna, lunaSubscribe } from "./luna";

/** pley for LG TV's built-in OAuth client (apps/server/src/oauth/builtin/webos.json). */
export const CLIENT_ID = "https://pley-media-server.github.io/pley/oauth/webos.json";

/** LG's remote: Back is 461; the rest are the standard codes. */
export function webosKey(e: KeyboardEvent): Key | null {
  switch (e.keyCode) {
    case 461:
      return "back";
    case 1536:
    case 1537:
      // The Magic Remote's pointer showing or hiding.
      return null;
    default:
      return standardKey(e);
  }
}

/**
 * pley for LG TVs: LG's pipeline behind a <video> element, LG's services
 * for what the TV is, the remote's keys. In a desktop browser (no Luna)
 * the same app runs for development, with the browser's own player.
 */
export function webosPlatform(): Platform {
  const onTv = hasLuna();
  let facts: WebosFacts = onTv ? { ...browserFacts(), model: "LG TV" } : browserFacts();
  const factsReady = onTv ? readFacts().then((f) => (facts = f)) : Promise.resolve(facts);
  return {
    name: "webos",
    clientId: CLIENT_ID,
    version: __PLEY_VERSION__,
    appName: "Pley for LG TV",
    storage: localStore,
    mapKey: webosKey,
    // A desktop browser (development) plays what it plays through a plain <video>.
    createEngine: (host) => (onTv ? new WebosEngine(host) : new VideoEngine(host, "Browser video")),
    async probe(overrides): Promise<Probe> {
      await factsReady;
      const r = onTv ? webosCaps(facts, overrides) : browserCaps();
      return { caps: r.caps, lines: r.lines, dvContainers: r.dvContainers };
    },
    device(): DeviceFacts {
      return {
        maker: "LG",
        model: facts.model,
        os: `webOS ${facts.webos}`,
        firmware: facts.firmware,
        year: null,
        chromium: facts.chromium,
      };
    },
    tier: () => tierOf(facts),
    async localAddress(): Promise<string | null> {
      if (!onTv) return null;
      const status = await luna("luna://com.webos.service.connectionmanager/getStatus", {});
      const wired = status.wired as { state?: string; ipAddress?: string } | undefined;
      const wifi = status.wifi as { state?: string; ipAddress?: string } | undefined;
      if (wired?.state === "connected" && wired.ipAddress) return wired.ipAddress;
      if (wifi?.state === "connected" && wifi.ipAddress) return wifi.ipAddress;
      return null;
    },
    keepAwake() {
      // LG keeps the screen on while its pipeline plays; nothing to ask.
    },
    exit() {
      const sys = window.webOSSystem ?? window.PalmSystem;
      if (sys?.platformBack) sys.platformBack();
      else window.close();
    },
    onVisibility(listener) {
      document.addEventListener("visibilitychange", () => listener(!document.hidden));
    },
    screenReader(listener) {
      // Audio Guidance (Settings › Accessibility), from the TV's settings service (webOS 3 and later).
      // In a desktop browser, `localStorage["pley.screenReader"] = "on"` stands in for it.
      if (!onTv) {
        if (localStore.get("pley.screenReader") === "on") listener(true);
        return;
      }
      lunaSubscribe("luna://com.webos.settingsservice/getSystemSettings", { category: "option", keys: ["audioGuidance"] }, (a) => {
        const settings = a.settings as { audioGuidance?: string } | undefined;
        if (settings && typeof settings.audioGuidance === "string") listener(settings.audioGuidance === "on");
      });
    },
    clock24() {
      try {
        const sample = new Date(2000, 0, 1, 15).toLocaleTimeString();
        return !/PM|AM/i.test(sample);
      } catch {
        return false;
      }
    },
  };
}
