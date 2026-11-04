import { describe, expect, it } from "vitest";
import type { PlayingSession, TrackOption } from "@/api/types";
import { makeItem, makeReceiver, makeSession } from "@/test/fixtures";
import {
  commandAnswered,
  estimatedPosition,
  newestOn,
  otherDevices,
  received,
  trackLabel,
  type RemoteSession,
  type SentCommand,
} from "./remote";

// The server's clock, in seconds, and this browser's, in milliseconds: they needn't agree.
const at = 1_700_000_000;
const seen = 1_800_000_000_000;
const report: PlayingSession = makeSession({
  position_ms: 60_000,
  duration_ms: 120_000,
  updated_at: at,
  controls: ["play", "pause", "seek", "skip", "next", "audio", "subtitles"],
  audio_index: 1,
  subtitle_index: -1,
});
const tv: RemoteSession = { ...report, reported_at_ms: seen };

/** The player's next report, `secs` after the last by both clocks. */
function later(secs: number, overrides: Partial<RemoteSession> = {}): RemoteSession {
  return { ...tv, updated_at: at + secs, reported_at_ms: seen + secs * 1000, ...overrides };
}

describe("received", () => {
  it("dates each report by this browser's clock, from how long ago the server says it was", () => {
    const [s] = received([{ ...report, age_ms: 4_000 }], seen);
    expect(s).toEqual({ ...report, age_ms: 4_000, reported_at_ms: seen - 4_000 });
  });
});

describe("estimatedPosition", () => {
  it("moves on from the last report while playing, up to the end", () => {
    expect(estimatedPosition(tv, seen + 15_000)).toBe(75_000);
    expect(estimatedPosition(tv, seen + 600_000)).toBe(120_000);
    // Never back before the report.
    expect(estimatedPosition(tv, seen - 5_000)).toBe(60_000);
    expect(estimatedPosition({ ...tv, duration_ms: null }, seen + 600_000)).toBe(660_000);
  });

  it("stays where it was when paused or buffering", () => {
    expect(estimatedPosition({ ...tv, state: "paused" }, seen + 15_000)).toBe(60_000);
    expect(estimatedPosition({ ...tv, state: "buffering" }, seen + 15_000)).toBe(60_000);
  });

  it("is the same whatever time the server's clock gave the report", () => {
    // A server clock minutes behind this browser's, the same, or minutes ahead.
    for (const updatedAt of [at - 300, at, at + 300]) {
      const [s] = received([{ ...report, updated_at: updatedAt, age_ms: 5_000 }], seen);
      expect(estimatedPosition(s!, seen + 10_000)).toBe(75_000);
    }
  });
});

describe("commandAnswered", () => {
  const sentAt = seen + 2_000;
  const sent = (command: SentCommand["command"], target?: number): SentCommand => ({ command, before: tv, sentAt, target });
  const by = sentAt + 1_000;

  it("sees a pause, another title, and a session gone", () => {
    expect(commandAnswered(sent("pause"), tv, by)).toBe(false);
    expect(commandAnswered(sent("pause"), later(2, { state: "paused" }), by)).toBe(true);
    expect(commandAnswered(sent("next"), { ...tv, item: makeItem({ id: 8 }) }, by)).toBe(true);
    expect(commandAnswered(sent("next"), undefined, by)).toBe(true);
  });

  it("sees a seek or skip in a report from somewhere new", () => {
    // Only the usual report since, where it would be by then.
    expect(commandAnswered(sent("skip", 92_000), later(4, { position_ms: 64_000 }), by)).toBe(false);
    expect(commandAnswered(sent("skip", 92_000), later(3, { position_ms: 93_000 }), by)).toBe(true);
    expect(commandAnswered(sent("seek", 10_000), later(3, { position_ms: 10_500 }), by)).toBe(true);
    // No report since says nothing of it.
    expect(commandAnswered(sent("seek", 10_000), tv, by)).toBe(false);
  });

  it("compares server times with server times and this browser's with its own, however far apart the clocks are", () => {
    // A browser a minute ahead of the server, or behind it.
    for (const skew of [60_000, -60_000]) {
      const before: RemoteSession = { ...tv, reported_at_ms: seen + skew };
      const off: SentCommand = { ...sent("skip", 92_000), before, sentAt: sentAt + skew };
      const next = (secs: number, position_ms: number) =>
        later(secs, { position_ms, reported_at_ms: before.reported_at_ms + secs * 1000 });
      expect(commandAnswered(off, next(3, 93_000), by + skew)).toBe(true);
      expect(commandAnswered(off, next(4, 64_000), by + skew)).toBe(false);
      expect(commandAnswered(off, before, by + skew)).toBe(false);
    }
  });

  it("sees a previous that starts the same title over", () => {
    expect(commandAnswered(sent("previous", 0), later(2, { position_ms: 0 }), by)).toBe(true);
    // A few seconds in, too near the start to have moved far.
    const early = { ...tv, position_ms: 4_000 };
    expect(commandAnswered({ ...sent("previous", 0), before: early }, later(2, { position_ms: 0 }), by)).toBe(true);
    expect(commandAnswered(sent("previous", 0), tv, by)).toBe(false);
  });

  it("sees a track taken once the player says it plays the one picked, a pause meanwhile aside", () => {
    const audio: SentCommand = { command: "audio", before: tv, sentAt, track: 2 };
    expect(commandAnswered(audio, later(2), by)).toBe(false);
    expect(commandAnswered(audio, later(2, { state: "paused" }), by)).toBe(false);
    expect(commandAnswered(audio, later(2, { audio_index: 2 }), by)).toBe(true);
    // Its own player took a new session for it: the device's newest says so.
    expect(commandAnswered(audio, later(2, { session_id: "s2", audio_index: 2 }), by)).toBe(true);

    const off: SentCommand = { command: "subtitles", before: { ...tv, subtitle_index: 3 }, sentAt, track: -1 };
    expect(commandAnswered(off, later(2, { subtitle_index: 3 }), by)).toBe(false);
    expect(commandAnswered(off, later(2, { subtitle_index: -1 }), by)).toBe(true);
    // Something else plays by now: nothing more to wait for.
    expect(commandAnswered(off, later(2, { item: makeItem({ id: 8 }) }), by)).toBe(true);
  });
});

describe("otherDevices", () => {
  const session = (id: string, device_id: string, device: string | null, more: Partial<PlayingSession> = {}): RemoteSession => ({
    ...makeSession({ session_id: id, device_id, device, ...more }),
    reported_at_ms: seen,
  });

  it("lists each device once, those that play first, then those ready to, each by name", () => {
    const devices = otherDevices(
      [
        makeReceiver({ receiver_id: "den", device_id: "app-2", name: "Den TV" }),
        makeReceiver({ receiver_id: "tv-1", device_id: "app-1", name: "Living room TV" }),
        makeReceiver({ receiver_id: "pad", device_id: "app-3", name: "Attic tablet", kind: "tablet" }),
      ],
      // A Jellyfin app plays without taking casts.
      [session("s1", "app-1", "Living room TV"), session("s2", "token-9", "Phone app", { device_kind: "other" })],
    );
    expect(devices.map((d) => [d.name, d.kind, d.receiver?.receiver_id ?? null, d.session?.session_id ?? null])).toEqual([
      ["Living room TV", "tv", "tv-1", "s1"],
      ["Phone app", "other", null, "s2"],
      ["Attic tablet", "tablet", "pad", null],
      ["Den TV", "tv", "den", null],
    ]);
  });

  it("shows a device's newest session, and names it as its receiver does, else as its session does", () => {
    const [device] = otherDevices(
      [makeReceiver({ name: null })],
      [session("old", "app-1", "Living room TV"), session("new", "app-1", "Living room TV")],
    );
    expect(device?.session?.session_id).toBe("new");
    expect(device?.name).toBe("Living room TV");
    expect(otherDevices([makeReceiver({ name: null })], []).map((d) => d.name)).toEqual(["Unknown device"]);
  });

  it("numbers devices of the same name", () => {
    const devices = otherDevices(
      [makeReceiver({ receiver_id: "tv-2", device_id: "app-2" })],
      [session("s1", "app-1", "Living room TV"), session("s3", "app-3", null)],
    );
    expect(devices.map((d) => d.name)).toEqual(["Living room TV (1)", "Unknown device", "Living room TV (2)"]);
  });
});

describe("newestOn", () => {
  it("is the device's newest session but those playing here", () => {
    const list = received([makeSession({ session_id: "a" }), makeSession({ session_id: "b" }), makeSession({ session_id: "c", device_id: "app-2" })], seen);
    expect(newestOn(list, "app-1", () => false)?.session_id).toBe("b");
    expect(newestOn(list, "app-1", (id) => id === "b")?.session_id).toBe("a");
    expect(newestOn(list, "app-3", () => false)).toBeUndefined();
  });
});

describe("trackLabel", () => {
  const track = (overrides: Partial<TrackOption>): TrackOption => ({
    index: 1,
    language: "eng",
    title: null,
    codec: null,
    channels: null,
    default: false,
    forced: false,
    ...overrides,
  });

  it("names tracks as the player's own menu does", () => {
    expect(trackLabel(track({ codec: "truehd", channels: 8 }), "audio")).toBe("English (TrueHD 7.1)");
    expect(trackLabel(track({ title: "Commentary", codec: "ac3", channels: 2 }), "audio")).toBe("English – Commentary (AC3 Stereo)");
    expect(trackLabel(track({ language: "fre", codec: "subrip", forced: true }), "subtitle")).toBe("French (SRT, Forced)");
    expect(trackLabel(track({ title: "English SDH", codec: "subrip" }), "subtitle")).toBe("English – SDH (SRT)");
    expect(trackLabel(track({ language: null }), "subtitle")).toBe("Unknown");
  });
});
