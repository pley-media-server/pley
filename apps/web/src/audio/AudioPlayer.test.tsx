import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useServerEvents } from "@/api/events";
import type { ItemSummary, PlaybackInfo, ServerEvent } from "@/api/types";
import { isPlayingHere } from "@/lib/playingHere";
import { makeItem } from "@/test/fixtures";
import { mockApi, renderWithProviders, testUser, type FakeRequest } from "@/test/render";
import { AudioPlayerProvider, useAudioPlayer } from "./AudioPlayer";

/** Stands in for the browser's EventSource: `send` delivers a server event. */
class FakeEventSource {
  static readonly CLOSED = 2;
  static last: FakeEventSource | null = null;
  readyState = 1;
  onopen: (() => void) | null = null;
  onmessage: ((msg: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  constructor() {
    FakeEventSource.last = this;
  }
  close() {
    this.readyState = FakeEventSource.CLOSED;
  }
  send(event: ServerEvent) {
    this.onmessage?.({ data: JSON.stringify(event) });
  }
}

const tracks: ItemSummary[] = [
  makeItem({ id: 21, kind: "track", title: "One", duration_ms: 200_000 }),
  makeItem({ id: 22, kind: "track", title: "Two", duration_ms: 180_000 }),
];

function session(id: string, durationMs: number): PlaybackInfo {
  return {
    session_id: id,
    method: "direct",
    url: `/api/playback/${id}/file`,
    hls: false,
    file_id: 1,
    duration_ms: durationMs,
    audio_index: null,
    subtitle_index: null,
    burn_subtitles: false,
    subtitles: [],
    reason: null,
  };
}

/** The server: each track played gets the next session, t1, t2… */
function serve(): FakeRequest[] {
  let n = 0;
  return mockApi({
    "POST /api/playback": (req) => {
      n++;
      const id = (req.body as { item_id: number }).item_id;
      return session(`t${n}`, tracks.find((t) => t.id === id)?.duration_ms ?? 0);
    },
    "POST /api/items/21/progress": () => undefined,
    "POST /api/items/22/progress": () => undefined,
    "DELETE /api/playback/t1": () => undefined,
    "DELETE /api/playback/t2": () => undefined,
  });
}

function Queue() {
  useServerEvents(testUser.id);
  const player = useAudioPlayer();
  return (
    <button type="button" onClick={() => player.playQueue(tracks)}>
      Play
    </button>
  );
}

const command = (
  command: Extract<ServerEvent, { type: "player_command" }>["command"],
  more: { session_id?: string; position_ms?: number; offset_ms?: number } = {},
): ServerEvent => ({
  type: "player_command",
  session_id: more.session_id ?? "t1",
  command,
  position_ms: more.position_ms ?? null,
  offset_ms: more.offset_ms ?? null,
  stream_index: null,
});
const send = (ev: ServerEvent) => act(() => FakeEventSource.last?.send(ev));
const starts = (requests: FakeRequest[]) => requests.filter((r) => r.method === "POST" && r.path === "/api/playback");
const reports = (requests: FakeRequest[], itemId = 21) =>
  requests.filter((r) => r.method === "POST" && r.path === `/api/items/${itemId}/progress`).map((r) => r.body);

/** The media element methods the player calls, which jsdom only stubs. */
const media = {
  play: vi.fn<() => Promise<void>>(),
  pause: vi.fn<() => void>(),
};

describe("AudioPlayer obeying the remote", () => {
  beforeEach(() => {
    vi.stubGlobal("EventSource", FakeEventSource);
    media.play = vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
    media.pause = vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => undefined);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  /** The queue, its first track playing (as far as it can tell). */
  async function playing() {
    const requests = serve();
    const { container } = renderWithProviders(
      <AudioPlayerProvider>
        <Queue />
      </AudioPlayerProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Play" }));
    const audio = container.querySelector("audio")!;
    await waitFor(() => expect(audio.getAttribute("src")).toBe("/api/playback/t1/file"));
    let paused = false;
    Object.defineProperty(audio, "paused", { configurable: true, get: () => paused });
    media.pause.mockClear();
    media.play.mockClear();
    return { requests, audio, setPaused: (p: boolean) => (paused = p) };
  }

  it("says what it obeys as a track starts, and plays it in this tab", async () => {
    const { requests } = await playing();
    // Neither audio nor subtitles: music has no tracks to pick.
    expect(starts(requests)[0]?.body).toMatchObject({
      item_id: 21,
      controls: ["play", "pause", "seek", "skip", "next", "previous"],
    });
    expect(isPlayingHere("t1")).toBe(true);
  });

  it("leaves a command for another session alone", async () => {
    const { requests, audio } = await playing();
    const before = requests.length;
    send(command("pause", { session_id: "other" }));
    send(command("next", { session_id: "other" }));
    send(command("seek", { session_id: "other", position_ms: 5_000 }));
    expect(media.pause).not.toHaveBeenCalled();
    expect(audio.currentTime).toBe(0);
    expect(requests.slice(before)).toEqual([]);
  });

  it("pauses and plays, and says it plays again at once", async () => {
    const { requests, setPaused } = await playing();
    send(command("pause"));
    expect(media.pause).toHaveBeenCalledTimes(1);

    setPaused(true);
    send(command("play"));
    expect(media.play).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ state: "playing", session_id: "t1" }));
  });

  it("says again that it plays, or is paused, when told so already, so a remote a little behind settles", async () => {
    const { requests, setPaused } = await playing();
    send(command("play"));
    expect(media.play).not.toHaveBeenCalled();
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ state: "playing", session_id: "t1" }));

    setPaused(true);
    send(command("pause"));
    expect(media.pause).not.toHaveBeenCalled();
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ state: "paused", session_id: "t1" }));
  });

  it("seeks, no further than the track's end, and reports where at once", async () => {
    const { requests, audio } = await playing();
    send(command("seek", { position_ms: 30_000 }));
    expect(audio.currentTime).toBe(30);
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ position_ms: 30_000, state: "playing" }));

    send(command("seek", { position_ms: 999_000 }));
    expect(audio.currentTime).toBe(200);
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ position_ms: 200_000 }));
  });

  it("skips from where it is, never before the start", async () => {
    const { requests, audio, setPaused } = await playing();
    send(command("seek", { position_ms: 30_000 }));
    setPaused(true);
    send(command("skip", { offset_ms: 15_000 }));
    expect(audio.currentTime).toBe(45);
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ position_ms: 45_000, state: "paused" }));

    send(command("skip", { offset_ms: -600_000 }));
    expect(audio.currentTime).toBe(0);
  });

  it("goes to the next track and back, and starts the first one over", async () => {
    const { requests, audio } = await playing();
    send(command("previous"));
    expect(starts(requests)).toHaveLength(1);
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ position_ms: 0, session_id: "t1" }));

    send(command("next"));
    await waitFor(() => expect(audio.getAttribute("src")).toBe("/api/playback/t2/file"));
    expect(starts(requests).at(-1)?.body).toMatchObject({ item_id: 22 });
    await waitFor(() => expect(isPlayingHere("t1")).toBe(false));

    // The last track: nothing comes next.
    send(command("next", { session_id: "t2" }));
    expect(starts(requests)).toHaveLength(2);

    send(command("previous", { session_id: "t2" }));
    await waitFor(() => expect(starts(requests).at(-1)?.body).toMatchObject({ item_id: 21 }));
  });
});
