import { afterEach, describe, expect, it } from "vitest";
import type { Api } from "../../src/api/client";
import type { Control, ItemSummary, PlaybackInfo, PlaybackRequest } from "../../src/api/types";
import { MusicPlayer } from "../../src/player/music";
import type { PlayerCommand } from "../../src/player/remote";
import { Reporter } from "../../src/player/reporter";
import type { AppState } from "../../src/state";

function track(id: number): ItemSummary {
  return { id, library_id: 3, kind: "track", title: `Track ${id}`, sort_title: `track ${id}`, added_at: 0, duration_ms: 240_000, user_state: { position_ms: 0, watched: false, play_count: 0 } };
}

/** The music player with the server and an <audio> element the test drives: what was asked, and where it plays. */
async function world(tracks: number[], at: number) {
  const calls: string[] = [];
  const requests: PlaybackRequest[] = [];
  let n = 0;
  const api = {
    url: (p: string) => `http://nas:7539${p}`,
    play: async (req: PlaybackRequest): Promise<PlaybackInfo> => {
      requests.push(req);
      n++;
      calls.push(`start ${req.item_id}`);
      return { session_id: `t${n}`, method: "direct", url: `/api/media/pley_mk_${n}/file.flac`, hls: false, file_id: req.item_id, duration_ms: 240_000, audio_index: 0, subtitle_index: null, burn_subtitles: false, subtitles: [], reason: null };
    },
    progress: async (id: number, pos: number, _d: number | null, state: string) => void calls.push(`progress ${id} ${state} ${Math.round(pos)}`),
    stopSession: async (id: string) => void calls.push(`delete ${id}`),
  } as unknown as Api;
  const app = {
    api: { value: api },
    reporter: new Reporter(),
    caps: async () => null,
    installId: () => "tizen-tv",
    onLeave: () => undefined,
  } as unknown as AppState;
  const player = new MusicPlayer(app);
  const audio = document.body.querySelector("audio:last-of-type") as HTMLAudioElement;
  const media = { paused: true, at: 0 };
  Object.defineProperty(audio, "paused", { get: () => media.paused });
  Object.defineProperty(audio, "currentTime", { get: () => media.at, set: (s: number) => (media.at = s) });
  audio.play = () => {
    media.paused = false;
    audio.dispatchEvent(new Event("playing"));
    return Promise.resolve();
  };
  audio.pause = () => {
    if (media.paused) return;
    media.paused = true;
    audio.dispatchEvent(new Event("pause"));
  };
  player.play(tracks.map(track), at, false);
  await settle();
  calls.length = 0;
  const send = (session: string, command: Control, value: { position_ms?: number; offset_ms?: number } = {}) => {
    const e: PlayerCommand = { type: "player_command", session_id: session, command, position_ms: value.position_ms ?? null, offset_ms: value.offset_ms ?? null };
    player.obey(e);
  };
  return { player, media, calls, requests, send };
}

const settle = () => new Promise((r) => setTimeout(r, 20));

afterEach(() => {
  document.body.innerHTML = "";
});

describe("music and the user's remote, on another device", () => {
  it("is told the music obeys every command", async () => {
    const { requests } = await world([1, 2, 3], 1);
    expect(requests[0]!.controls).toEqual(["play", "pause", "seek", "skip", "next", "previous"]);
  });

  it("pauses and plays the track's session, reporting each at once, and leaves another session's commands alone", async () => {
    const { media, calls, send } = await world([1, 2, 3], 1);
    media.at = 60;
    send("t9", "pause");
    expect(media.paused).toBe(false);
    send("t1", "pause");
    send("t1", "play");
    await settle();
    expect(calls).toEqual(["progress 2 paused 60000", "progress 2 playing 60000"]);
  });

  it("says again that it plays, or is paused, when told so already, so a remote a little behind settles", async () => {
    const { media, calls, send } = await world([1, 2, 3], 1);
    media.at = 60;
    send("t1", "play");
    send("t1", "pause");
    send("t1", "pause");
    await settle();
    expect(media.paused).toBe(true);
    expect(calls).toEqual(["progress 2 playing 60000", "progress 2 paused 60000", "progress 2 paused 60000"]);
  });

  it("seeks within the track and skips from where it is, reporting where it went", async () => {
    const { media, calls, send } = await world([1, 2, 3], 1);
    media.at = 60;
    send("t1", "skip", { offset_ms: -90_000 });
    send("t1", "seek", { position_ms: 300_000 });
    await settle();
    expect(calls).toEqual(["progress 2 playing 0", "progress 2 playing 240000"]);
  });

  it("starts the track again past 3 s, else plays the one before, and plays the next while there is one", async () => {
    const w = await world([1, 2, 3], 1);
    w.media.at = 10;
    w.send("t1", "previous");
    await settle();
    expect(w.media.at).toBe(0);
    expect(w.calls).toEqual(["progress 2 playing 0"]);
    w.calls.length = 0;
    w.send("t1", "previous");
    await settle();
    expect(w.calls).toEqual(["progress 2 stopped 0", "delete t1", "start 1"]);
    w.calls.length = 0;
    w.send("t2", "next");
    await settle();
    expect(w.calls).toEqual(["progress 1 stopped 0", "delete t2", "start 2"]);
    const last = await world([1, 2], 1);
    last.send("t1", "next");
    await settle();
    expect(last.calls).toEqual([]);
  });
});
