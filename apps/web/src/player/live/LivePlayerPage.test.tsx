import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from "vitest";
import { useServerEvents } from "@/api/events";
import type { Channel } from "@/api/livetv";
import type { ServerEvent } from "@/api/types";
import { isPlayingHere } from "@/lib/playingHere";
import { makeReceiver } from "@/test/fixtures";
import { apiError, LocationDisplay, mockApi, renderWithProviders, testUser, type FakeRequest } from "@/test/render";
import { LivePlayerPage } from "./LivePlayerPage";

/** A stand-in for hls.js: records its calls, and fires events on demand. */
const { FakeHls, hlsInstances } = vi.hoisted(() => {
  type Handler = (event: string, data: unknown) => void;
  const hlsInstances: FakeHls[] = [];
  class FakeHls {
    static isSupported = () => true;
    static Events = { ERROR: "hlsError", MANIFEST_PARSED: "hlsManifestParsed" };
    static ErrorTypes = { NETWORK_ERROR: "networkError", MEDIA_ERROR: "mediaError" };
    readonly calls: string[] = [];
    private readonly handlers = new Map<string, Handler[]>();
    constructor() {
      hlsInstances.push(this);
    }
    on(event: string, handler: Handler) {
      this.handlers.set(event, [...(this.handlers.get(event) ?? []), handler]);
    }
    emit(event: string, data: unknown) {
      for (const handler of this.handlers.get(event) ?? []) handler(event, data);
    }
    loadSource(url: string) {
      this.calls.push(`loadSource ${url}`);
    }
    attachMedia() {
      this.calls.push("attachMedia");
    }
    startLoad() {
      this.calls.push("startLoad");
    }
    recoverMediaError() {
      this.calls.push("recoverMediaError");
    }
    destroy() {
      this.calls.push("destroy");
    }
  }
  return { FakeHls, hlsInstances };
});

/**
 * A stand-in for mpegts.js's player, as Chrome's goes with a channel whose
 * audio it can't play (AC-3): it fails as it loads (or, with `failsAsItLoads`
 * off, when `fail` says), and throws when let go twice, as the real one does.
 * `comes` reports the stream coming, as the real one does every 600 ms;
 * `found` says what it read in the stream, as the real one does once.
 */
const { FakeMpegts, mpegtsPlayers, mpegtsLoader } = vi.hoisted(() => {
  const mpegtsPlayers: FakeMpegtsPlayer[] = [];
  class FakeMpegtsPlayer {
    static failsAsItLoads = true;
    readonly calls: string[] = [];
    private readonly handlers = new Map<string, (info?: unknown) => void>();
    private engine: object | null = {};
    constructor() {
      mpegtsPlayers.push(this);
    }
    on(event: string, handler: (info?: unknown) => void) {
      this.handlers.set(event, handler);
    }
    comes(kbps: number) {
      this.handlers.get("statistics_info")?.({ speed: kbps });
    }
    found(tracks: unknown) {
      this.handlers.get("media_info")?.(tracks);
    }
    attachMediaElement() {
      this.calls.push("attachMediaElement");
    }
    load() {
      this.calls.push("load");
      if (FakeMpegtsPlayer.failsAsItLoads) this.fail();
    }
    fail() {
      this.handlers.get("error")?.();
    }
    destroy() {
      if (!this.engine) throw new TypeError("Cannot read properties of null (reading 'destroy')");
      this.engine = null;
      this.calls.push("destroy");
    }
  }
  const FakeMpegts = {
    Player: FakeMpegtsPlayer,
    isSupported: () => true,
    Events: { ERROR: "error", STATISTICS_INFO: "statistics_info", MEDIA_INFO: "media_info" },
    createPlayer: () => new FakeMpegtsPlayer(),
  };
  // No MPEG-TS player unless a test gives one: every channel plays from the transcoder's HLS.
  const mpegtsLoader = { load: (): Promise<unknown> => Promise.reject(new Error("not here")) };
  return { FakeMpegts, mpegtsPlayers, mpegtsLoader };
});

vi.mock("@/lib/hls", () => ({ loadHls: () => Promise.resolve(FakeHls) }));
vi.mock("@/lib/mpegts", () => ({ loadMpegts: () => mpegtsLoader.load() }));

const news: Channel = {
  id: 5,
  source_id: 1,
  source_uid: "src",
  name: "News",
  number: "5",
  group: "News",
  logo: null,
  favorite: false,
  has_guide: false,
  now: null,
  next: null,
};

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

/** The live player with the app's event stream open, as signed in. */
function PlayerWithEvents() {
  useServerEvents(testUser.id);
  return (
    <Routes>
      <Route path="/live/:channelId" element={<LivePlayerPage />} />
      <Route path="/live/:sourceUid/:channelId" element={<LivePlayerPage />} />
    </Routes>
  );
}

function serve(): FakeRequest[] {
  let n = 0;
  return mockApi({
    "GET /api/live/channels/5": () => news,
    "POST /api/live/channels/5/watch": () => {
      n++;
      return { session_id: `s${n}`, url: `/api/live/sessions/s${n}/index.m3u8`, kind: "hls", channel: news };
    },
    "DELETE /api/live/sessions/s1": () => undefined,
    "DELETE /api/live/sessions/s2": () => undefined,
  });
}

const watches = (requests: FakeRequest[]) =>
  requests.filter((r) => r.method === "POST" && r.path === "/api/live/channels/5/watch");

const takenOver = (channel_id: number): ServerEvent => ({
  type: "live_taken_over",
  session_id: "s1",
  channel_id,
  message: "Someone else needed this channel's connection to watch another channel.",
});

describe("LivePlayerPage, its connection taken over", () => {
  beforeEach(() => {
    hlsInstances.length = 0;
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => undefined);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("stops at once, says why, and watches again only when asked", async () => {
    const requests = serve();
    renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));

    // Another channel's: nothing happens here.
    act(() => FakeEventSource.last?.send(takenOver(6)));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    act(() => FakeEventSource.last?.send(takenOver(5)));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Live TV stopped");
    expect(alert).toHaveTextContent("Someone else needed this channel's connection to watch another channel.");
    expect(hlsInstances[0]?.calls.at(-1)).toBe("destroy");

    // The stream's own 410 that follows says nothing else.
    act(() => hlsInstances[0]?.emit("hlsError", { fatal: true, type: "networkError", response: { code: 410 } }));
    expect(screen.getByRole("alert")).toHaveTextContent("Someone else needed");
    expect(watches(requests)).toHaveLength(1);
    // Ended on the server: nothing to leave.
    expect(requests.some((r) => r.method === "DELETE")).toBe(false);

    await userEvent.click(screen.getByRole("button", { name: "Watch again" }));
    await waitFor(() => expect(watches(requests)).toHaveLength(2));
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });

  it("lets the browser's own HLS go, so it doesn't play on", async () => {
    serve();
    // A browser that plays HLS itself (Safari before hls.js), whose pause
    // fires its event as a real one does.
    const supported = FakeHls.isSupported;
    FakeHls.isSupported = () => false;
    vi.spyOn(HTMLMediaElement.prototype, "canPlayType").mockImplementation((type) =>
      type === "application/vnd.apple.mpegurl" ? "maybe" : "",
    );
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(function (this: HTMLMediaElement) {
      this.dispatchEvent(new Event("pause"));
    });
    try {
      const { container } = renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
      const video = await waitFor(() => {
        const el = container.querySelector("video");
        expect(el?.getAttribute("src")).toBe("/api/live/sessions/s1/index.m3u8");
        return el!;
      });
      const play = vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
      play.mockClear();
      act(() => FakeEventSource.last?.send(takenOver(5)));
      expect(await screen.findByRole("alert")).toHaveTextContent("Live TV stopped");
      expect(video.hasAttribute("src")).toBe(false);
      // Live TV's "plays on" after a pause (100 ms) finds nothing to play.
      await new Promise((resolve) => setTimeout(resolve, 300));
      expect(play).not.toHaveBeenCalled();
      expect(screen.getByRole("alert")).toHaveTextContent("Live TV stopped");
    } finally {
      FakeHls.isSupported = supported;
    }
  });

  it("says the server's words when its stream answers 410", async () => {
    serve();
    renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    const body = JSON.stringify({ error: "session_stopped", message: "Watching moved to Sport on another device." });
    act(() =>
      hlsInstances[0]?.emit("hlsError", { fatal: true, type: "networkError", response: { code: 410, text: body } }),
    );
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Live TV stopped");
    expect(alert).toHaveTextContent("Watching moved to Sport on another device.");
  });
});

describe("LivePlayerPage, a channel played from the transcoder's segments", () => {
  beforeEach(() => {
    hlsInstances.length = 0;
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => undefined);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("plays on, with its sound, after a media error it recovers from", async () => {
    // hls.js loads the video again as it recovers, which cuts short the play under way.
    const play = vi
      .spyOn(HTMLMediaElement.prototype, "play")
      .mockRejectedValueOnce(new DOMException("cut short", "AbortError"))
      .mockResolvedValue();
    serve();
    const { container } = renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    act(() => hlsInstances[0]?.emit("hlsManifestParsed", {}));
    expect(play).toHaveBeenCalledTimes(1);
    act(() => hlsInstances[0]?.emit("hlsError", { fatal: true, type: "mediaError", details: "bufferAppendError" }));
    expect(hlsInstances[0]?.calls).toContain("recoverMediaError");
    expect(play).toHaveBeenCalledTimes(2);
    await act(() => Promise.resolve());
    expect(container.querySelector("video")!.muted).toBe(false);
  });
});

describe("LivePlayerPage, a channel the browser can't play as sent", () => {
  beforeEach(() => {
    hlsInstances.length = 0;
    mpegtsPlayers.length = 0;
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.stubGlobal("MediaSource", class {});
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => undefined);
  });
  afterEach(() => {
    mpegtsLoader.load = () => Promise.reject(new Error("not here"));
    FakeMpegts.Player.failsAsItLoads = true;
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("plays the transcoder's segments when its stream fails", async () => {
    mpegtsLoader.load = () => Promise.resolve(FakeMpegts);
    mockApi({
      "GET /api/live/channels/5": () => news,
      "POST /api/live/channels/5/watch": () => ({ session_id: "s1", url: "/api/live/sessions/s1/index.m3u8", kind: "hls", channel: news }),
      // A broadcast channel: MPEG-2 and AC-3, made into H.264 and AAC.
      "GET /api/live/sessions/s1/plan": () => ({ video_codec: "mpeg2video", audio_codec: "ac3", video: "h264", audio: "aac" }),
      "DELETE /api/live/sessions/s1": () => undefined,
    });
    renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(hlsInstances[0]?.calls).toEqual(["loadSource /api/live/sessions/s1/index.m3u8", "attachMedia"]));
    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("plays the transcoder's segments when its stream fails before its first picture", async () => {
    mpegtsLoader.load = () => Promise.resolve(FakeMpegts);
    FakeMpegts.Player.failsAsItLoads = false;
    const play = vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
    // The stream's first seconds come: playback starts once they're buffered.
    vi.spyOn(HTMLMediaElement.prototype, "buffered", "get").mockReturnValue({
      length: 1,
      start: () => 0,
      end: () => 3,
    });
    let n = 0;
    const requests = mockApi({
      "GET /api/live/channels/5": () => news,
      "POST /api/live/channels/5/watch": () => {
        n++;
        return { session_id: `s${n}`, url: `/api/live/sessions/s${n}/index.m3u8`, kind: "hls", channel: news };
      },
      // H.264 and AAC: the server says it's sent as it is, before the stream breaks off.
      "GET /api/live/sessions/s1/plan": () => ({ video_codec: "h264", audio_codec: "aac", video: "copy", audio: "copy" }),
      "DELETE /api/live/sessions/s1": () => undefined,
      "DELETE /api/live/sessions/s2": () => undefined,
    });
    renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(play).toHaveBeenCalled());
    expect(hlsInstances).toHaveLength(0);

    act(() => mpegtsPlayers[0]?.fail());
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("loadSource /api/live/sessions/s2/index.m3u8"), {
      timeout: 3000,
    });
    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
    expect(watches(requests)).toHaveLength(2);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("stays stopped when watching ends before the server says what the channel is", async () => {
    mpegtsLoader.load = () => Promise.resolve(FakeMpegts);
    FakeMpegts.Player.failsAsItLoads = false;
    let sendPlan: (plan: unknown) => void = () => undefined;
    const requests = mockApi({
      "GET /api/live/channels/5": () => news,
      "POST /api/live/channels/5/watch": () => ({ session_id: "s1", url: "/api/live/sessions/s1/index.m3u8", kind: "hls", channel: news }),
      "GET /api/live/sessions/s1/plan": () => new Promise((resolve) => (sendPlan = resolve)),
    });
    renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(requests.some((r) => r.path === "/api/live/sessions/s1/plan")).toBe(true));

    act(() => FakeEventSource.last?.send(takenOver(5)));
    expect(await screen.findByRole("alert")).toHaveTextContent("Live TV stopped");
    // A broadcast channel, which would have played from the transcoder's segments.
    act(() => sendPlan({ video_codec: "mpeg2video", audio_codec: "ac3", video: "h264", audio: "aac" }));
    await act(() => new Promise((resolve) => setTimeout(resolve, 100)));

    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
    expect(hlsInstances).toHaveLength(0);
    expect(screen.getByRole("alert")).toHaveTextContent("Someone else needed");
  });
});

describe("LivePlayerPage, a channel played from its stream", () => {
  // Where what's buffered ends, how ready the video says it is, and where it plays.
  let ahead = 0;
  let ready = 0;
  let time = 0;
  let requests: FakeRequest[] = [];
  // What the server says the channel is, asked as it tunes and again before its stream is read again.
  const sentAsItIs = { video_codec: "h264", audio_codec: "aac", video: "copy", audio: "copy" };
  let planAnswer: () => unknown = () => sentAsItIs;
  beforeEach(() => {
    planAnswer = () => sentAsItIs;
    vi.useFakeTimers({ shouldAdvanceTime: true });
    hlsInstances.length = 0;
    mpegtsPlayers.length = 0;
    time = 0;
    mpegtsLoader.load = () => Promise.resolve(FakeMpegts);
    FakeMpegts.Player.failsAsItLoads = false;
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.stubGlobal("MediaSource", class {});
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => undefined);
    vi.spyOn(HTMLMediaElement.prototype, "buffered", "get").mockImplementation(() => ({
      length: 1,
      start: () => 0,
      end: () => ahead,
    }));
    vi.spyOn(HTMLMediaElement.prototype, "readyState", "get").mockImplementation(() => ready);
    vi.spyOn(HTMLMediaElement.prototype, "currentTime", "get").mockImplementation(() => time);
    requests = mockApi({
      "GET /api/live/channels/5": () => news,
      "POST /api/live/channels/5/watch": () => ({ session_id: "s1", url: "/api/live/sessions/s1/index.m3u8", kind: "hls", channel: news }),
      "GET /api/live/sessions/s1/plan": () => planAnswer(),
      "DELETE /api/live/sessions/s1": () => undefined,
    });
  });
  afterEach(() => {
    vi.useRealTimers();
    mpegtsLoader.load = () => Promise.reject(new Error("not here"));
    FakeMpegts.Player.failsAsItLoads = true;
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  /** Plays the channel with its first seconds buffered; the video and the stall spinner. */
  async function playing() {
    const play = vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
    ahead = 3;
    ready = HTMLMediaElement.HAVE_ENOUGH_DATA;
    const { container } = renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(play).toHaveBeenCalled());
    const video = container.querySelector("video")!;
    act(() => {
      video.dispatchEvent(new Event("playing"));
    });
    const spinner = () => container.querySelector(".animate-spin");
    expect(spinner()).not.toBeInTheDocument();
    return { video, spinner, play };
  }

  /** It runs out: nothing left to play. */
  function stall(video: HTMLVideoElement) {
    ahead = 0;
    ready = HTMLMediaElement.HAVE_CURRENT_DATA;
    act(() => {
      video.dispatchEvent(new Event("waiting"));
    });
  }

  it("shows no spinner when the cushion comes back at once", async () => {
    const { video, spinner } = await playing();
    stall(video);
    expect(video.playbackRate).toBe(0);
    // The provider's next burst, more than the cushion, before the spinner was due.
    ahead = 5;
    ready = HTMLMediaElement.HAVE_ENOUGH_DATA;
    act(() => {
      video.dispatchEvent(new Event("playing"));
      vi.advanceTimersByTime(300);
    });
    expect(video.playbackRate).toBe(1);
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(spinner()).not.toBeInTheDocument();
  });

  it("keeps the spinner while nothing comes, after waiting its longest too", async () => {
    const { video, spinner } = await playing();
    stall(video);
    expect(spinner()).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(6500);
    });
    // Gone on, with nothing to play yet.
    expect(video.playbackRate).toBe(1);
    expect(spinner()).toBeInTheDocument();
    // Then something comes, and it plays.
    ahead = 1;
    ready = HTMLMediaElement.HAVE_ENOUGH_DATA;
    act(() => {
      video.dispatchEvent(new Event("canplay"));
    });
    expect(spinner()).not.toBeInTheDocument();
  });

  /** The stream comes for `seconds`, as the player says every 600 ms, and none of it is buffered. */
  function comesForNothing(seconds: number) {
    for (let ms = 0; ms < seconds * 1000; ms += 600) {
      act(() => {
        mpegtsPlayers.at(-1)?.comes(320);
        vi.advanceTimersByTime(600);
      });
    }
  }

  /** It plays on for three seconds, the stream buffered ahead. */
  function playsOn() {
    for (let i = 0; i < 3; i++) {
      time += 1;
      ahead = time + 3;
      ready = HTMLMediaElement.HAVE_ENOUGH_DATA;
      act(() => {
        vi.advanceTimersByTime(1000);
      });
    }
  }

  const plans = (r: FakeRequest[]) => r.filter((q) => q.method === "GET" && q.path === "/api/live/sessions/s1/plan");
  const toHls = () =>
    waitFor(() => expect(hlsInstances[0]?.calls).toContain("loadSource /api/live/sessions/s1/index.m3u8"));

  /** It plays, then stands still for good (the provider's clock started over) while the stream comes: read again. */
  async function readAgain() {
    const shown = await playing();
    playsOn();
    stall(shown.video);
    comesForNothing(6);
    expect(mpegtsPlayers).toHaveLength(1);
    comesForNothing(4);
    await waitFor(() => expect(mpegtsPlayers).toHaveLength(2));
    // The new player's video starts again at 0.
    time = 0;
    return shown;
  }

  it("reads its stream again, on the same session, when it stands still while the stream comes", async () => {
    const { video, spinner, play } = await readAgain();
    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
    expect(mpegtsPlayers[1]?.calls).toEqual(["attachMediaElement", "load"]);
    // Its session seen first, so it stays while the stream is read again; nothing tuned again.
    expect(plans(requests)).toHaveLength(2);
    expect(watches(requests)).toHaveLength(1);
    expect(hlsInstances).toHaveLength(0);
    // The spinner stays while the new player fills its cushion…
    play.mockClear();
    ahead = 1;
    ready = HTMLMediaElement.HAVE_ENOUGH_DATA;
    act(() => {
      video.dispatchEvent(new Event("canplay"));
      vi.advanceTimersByTime(400);
    });
    expect(spinner()).toBeInTheDocument();
    expect(play).not.toHaveBeenCalled();
    // …and goes as it plays.
    ahead = 2.5;
    act(() => {
      vi.advanceTimersByTime(400);
    });
    expect(play).toHaveBeenCalled();
    act(() => {
      video.dispatchEvent(new Event("playing"));
    });
    expect(spinner()).not.toBeInTheDocument();
  });

  it("plays the transcoder's segments when the stream read again stands still without playing", async () => {
    await readAgain();
    comesForNothing(10);
    await toHls();
    expect(mpegtsPlayers).toHaveLength(2);
    expect(mpegtsPlayers[1]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
  });

  it("plays the transcoder's segments when it stands still again soon after it was read again", async () => {
    const { video } = await readAgain();
    ahead = 3;
    act(() => {
      vi.advanceTimersByTime(400);
    });
    playsOn();
    // A provider whose clock starts over at every break: still again a minute later.
    stall(video);
    comesForNothing(10);
    await toHls();
    expect(mpegtsPlayers).toHaveLength(2);
    expect(mpegtsPlayers[1]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
  });

  it("reads its stream again once more when it stands still again, two minutes on", async () => {
    const { video } = await readAgain();
    ahead = 3;
    act(() => {
      vi.advanceTimersByTime(400);
    });
    // A provider whose clock starts over now and then, not at every break.
    for (let i = 0; i < 41; i++) playsOn();
    stall(video);
    comesForNothing(10);
    await waitFor(() => expect(mpegtsPlayers).toHaveLength(3));
    expect(plans(requests)).toHaveLength(3);
    expect(hlsInstances).toHaveLength(0);
  });

  it("waits, without reading its stream again, while the stream doesn't come", async () => {
    const { video, spinner } = await playing();
    playsOn();
    stall(video);
    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    expect(mpegtsPlayers).toHaveLength(1);
    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load"]);
    expect(spinner()).toBeInTheDocument();
  });

  it("doesn't read again a stream that came, then ended", async () => {
    const { video } = await playing();
    playsOn();
    stall(video);
    // The stream ends (the channel stopped on the server): the player says nothing more.
    comesForNothing(3);
    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    expect(mpegtsPlayers).toHaveLength(1);
    expect(plans(requests)).toHaveLength(1);
    expect(hlsInstances).toHaveLength(0);
  });

  it("stops, and says why, when its session ended as its stream would be read again", async () => {
    const { video } = await playing();
    planAnswer = () => apiError(410, "session_stopped", "An administrator stopped this channel.");
    playsOn();
    stall(video);
    comesForNothing(10);
    expect(await screen.findByRole("alert")).toHaveTextContent("An administrator stopped this channel.");
    expect(mpegtsPlayers).toHaveLength(1);
    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
    expect(watches(requests)).toHaveLength(1);
    expect(hlsInstances).toHaveLength(0);
  });

  it.each([
    ["refuses sound", "NotAllowedError", true],
    ["cuts a play short", "AbortError", false],
  ])("starts muted only when the browser refuses sound: it %s", async (_, name, muted) => {
    const play = vi
      .spyOn(HTMLMediaElement.prototype, "play")
      .mockRejectedValueOnce(new DOMException("refused", name))
      .mockResolvedValue();
    ahead = 3;
    ready = HTMLMediaElement.HAVE_ENOUGH_DATA;
    const { container } = renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(play).toHaveBeenCalled());
    await act(() => Promise.resolve());
    expect(container.querySelector("video")!.muted).toBe(muted);
  });
});

describe("LivePlayerPage, a channel this browser decodes as its provider sends it", () => {
  const MAIN = 'video/mp4; codecs="hvc1.1.6.L120.90"';
  const MAIN_10 = 'video/mp4; codecs="hvc1.2.4.L153.B0"';
  const AAC = 'audio/mp4; codecs="mp4a.40.2"';
  // An HEVC channel with AAC sound: the transcoder would make its video H.264.
  const hevc = {
    video_codec: "hevc",
    width: 1920,
    height: 1080,
    frame_rate: 50,
    interlaced: false,
    bit_depth: 10,
    audio_codec: "aac",
    audio_channels: 2,
    video: "h264_nvenc",
    audio: "copy",
    out_height: 1080,
    video_kbps: 6000,
      kbps: 5200,
  };
  // The stream as mpegts.js reads it: its HEVC by the profile and level it has.
  const itsTracks = { hasVideo: true, hasAudio: true, videoCodec: "hvc1.2.1.L123.B0", audioCodec: "mp4a.40.2" };
  /** What Media Source Extensions take here. */
  let decoded: string[] = [];
  let planAnswer: () => unknown = () => hevc;
  let requests: FakeRequest[] = [];
  let play: MockInstance<HTMLMediaElement["play"]>;

  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    hlsInstances.length = 0;
    mpegtsPlayers.length = 0;
    planAnswer = () => hevc;
    decoded = [MAIN, MAIN_10, 'video/mp4; codecs="hvc1.2.1.L123.B0"', AAC];
    mpegtsLoader.load = () => Promise.resolve(FakeMpegts);
    FakeMpegts.Player.failsAsItLoads = false;
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.stubGlobal("MediaSource", { isTypeSupported: (type: string) => decoded.includes(type) });
    play = vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => undefined);
    // The stream's first seconds come: playback starts once they're buffered.
    vi.spyOn(HTMLMediaElement.prototype, "buffered", "get").mockReturnValue({ length: 1, start: () => 0, end: () => 3 });
    let n = 0;
    const plan = () => planAnswer();
    requests = mockApi({
      "GET /api/live/channels/5": () => news,
      "POST /api/live/channels/5/watch": () => {
        n++;
        return { session_id: `s${n}`, url: `/api/live/sessions/s${n}/index.m3u8`, kind: "hls", channel: news };
      },
      "GET /api/live/sessions/s1/plan": plan,
      "GET /api/live/sessions/s2/plan": plan,
      "DELETE /api/live/sessions/s1": () => undefined,
      "DELETE /api/live/sessions/s2": () => undefined,
    });
  });
  afterEach(() => {
    vi.useRealTimers();
    mpegtsLoader.load = () => Promise.reject(new Error("not here"));
    FakeMpegts.Player.failsAsItLoads = true;
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  const plans = () => requests.filter((r) => r.method === "GET" && r.path.endsWith("/plan"));
  /** The channel tunes: its stream is read, and the server has said what it is. */
  async function tuned() {
    const shown = renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(mpegtsPlayers[0]?.calls).toContain("load"));
    await waitFor(() => expect(plans()).toHaveLength(1));
    await act(() => vi.advanceTimersByTimeAsync(50));
    return shown;
  }
  const toHls = (session = "s1") =>
    waitFor(() => expect(hlsInstances[0]?.calls).toContain(`loadSource /api/live/sessions/${session}/index.m3u8`));

  it("plays HEVC from its stream once mpegts.js has found its picture and sound", async () => {
    await tuned();
    // Not before: the server doesn't say how the stream carries them.
    expect(play).not.toHaveBeenCalled();

    act(() => mpegtsPlayers[0]?.found(itsTracks));
    await waitFor(() => expect(play).toHaveBeenCalled());
    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load"]);
    expect(hlsInstances).toHaveLength(0);
  });

  it("plays it at once when its tracks were found before the server said what it is", async () => {
    let sendPlan: (plan: unknown) => void = () => undefined;
    planAnswer = () => new Promise((resolve) => (sendPlan = resolve));
    renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(mpegtsPlayers[0]?.calls).toContain("load"));
    await waitFor(() => expect(plans()).toHaveLength(1));
    act(() => mpegtsPlayers[0]?.found(itsTracks));
    expect(play).not.toHaveBeenCalled();

    act(() => sendPlan(hevc));
    await waitFor(() => expect(play).toHaveBeenCalled());
    expect(hlsInstances).toHaveLength(0);
  });

  it("plays the transcoder's segments when mpegts.js doesn't find them in time", async () => {
    await tuned();
    await act(() => vi.advanceTimersByTimeAsync(1500));
    expect(hlsInstances).toHaveLength(0);
    await act(() => vi.advanceTimersByTimeAsync(1000));
    await toHls();
    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
  });

  it("plays the transcoder's segments when the stream's own profile isn't decoded", async () => {
    await tuned();
    // Its HEVC turns out to be one this browser has no decoder for.
    act(() => mpegtsPlayers[0]?.found({ ...itsTracks, videoCodec: "hvc1.4.1.L153.B0" }));
    await toHls();
    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
  });

  it("plays the transcoder's segments when its stream fails while its tracks are waited for", async () => {
    await tuned();
    act(() => mpegtsPlayers[0]?.fail());
    await toHls();
    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
  });

  it("falls back to the transcoder's segments when its stream breaks off as it plays", async () => {
    await tuned();
    act(() => mpegtsPlayers[0]?.found(itsTracks));
    await waitFor(() => expect(play).toHaveBeenCalled());

    act(() => mpegtsPlayers[0]?.fail());
    await act(() => vi.advanceTimersByTimeAsync(1500));
    await toHls("s2");
  });

  it.each([
    ["its video is interlaced", () => ({ ...hevc, interlaced: true })],
    ["its sound isn't decoded", () => ({ ...hevc, audio_codec: "ac3", audio: "aac" })],
  ])("plays the transcoder's segments at once when %s", async (_why, answer) => {
    planAnswer = answer;
    await tuned();
    await toHls();
    expect(play).not.toHaveBeenCalled();
    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
  });

  it("plays the transcoder's segments at once without an HEVC decoder for its bit depth", async () => {
    decoded = [MAIN, AAC];
    await tuned();
    await toHls();
    expect(mpegtsPlayers[0]?.calls).toEqual(["attachMediaElement", "load", "destroy"]);
  });

  it("leaves a channel with sound alone that the server converts to the transcoder", async () => {
    // mpegts.js never says what it found in a stream with one track.
    decoded = [...decoded, "audio/mpeg", 'audio/mp4; codecs="mp3"'];
    planAnswer = () => ({ ...hevc, video_codec: null, video: "none", audio_codec: "mp3", audio: "aac" });
    await tuned();
    await toHls();
  });

  /** Tunes it with the server's plan held back: the stream's tracks are found, then no plan comes in time. */
  async function tunedWithoutPlan() {
    hlsInstances.length = 0;
    mpegtsPlayers.length = 0;
    play.mockClear();
    let sendPlan: (plan: unknown) => void = () => undefined;
    planAnswer = () => new Promise((resolve) => (sendPlan = resolve));
    const asked = plans().length;
    renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(mpegtsPlayers[0]?.calls).toContain("load"));
    await waitFor(() => expect(plans()).toHaveLength(asked + 1));
    act(() => mpegtsPlayers[0]?.found(itsTracks));
    act(() => sendPlan(null));
  }

  it("plays it by the tracks found when its plan doesn't come in time", async () => {
    await tunedWithoutPlan();
    await waitFor(() => expect(play).toHaveBeenCalled());
    expect(hlsInstances).toHaveLength(0);
  });

  describe("and its Playback details", () => {
    const details = () => screen.getByRole("region", { name: "Playback details" });
    const open = () => fireEvent.click(screen.getByRole("button", { name: "Playback details" }));

    it("say it plays directly, and what the provider sends, asked again while they show", async () => {
      await tuned();
      act(() => mpegtsPlayers[0]?.found(itsTracks));
      await waitFor(() => expect(play).toHaveBeenCalled());
      // The badge says how it plays, as the video player's does.
      expect(screen.getByRole("button", { name: "Playback details" })).toHaveTextContent("Direct play");
      expect(screen.queryByRole("region", { name: "Playback details" })).not.toBeInTheDocument();
      // Closed, nothing is asked again.
      await act(() => vi.advanceTimersByTimeAsync(6000));
      expect(plans()).toHaveLength(1);

      open();
      expect(within(details()).getByText("Direct play")).toBeInTheDocument();
      expect(within(details()).getByText("HEVC 10-bit · 1920×1080 · 50 fps")).toBeInTheDocument();
      expect(within(details()).getByText("AAC Stereo")).toBeInTheDocument();
      expect(within(details()).getByText("5.2 Mbps")).toBeInTheDocument();
      expect(within(details()).queryByText("Encoder")).not.toBeInTheDocument();

      planAnswer = () => ({ ...hevc, kbps: 6400 });
      await act(() => vi.advanceTimersByTimeAsync(3000));
      expect(await within(details()).findByText("6.4 Mbps")).toBeInTheDocument();
      const asked = plans().length;
      expect(asked).toBeGreaterThan(1);

      // Closed again (Escape closes them before it leaves the player): no more asking.
      fireEvent.keyDown(window, { key: "Escape" });
      expect(screen.queryByRole("region", { name: "Playback details" })).not.toBeInTheDocument();
      await act(() => vi.advanceTimersByTimeAsync(6000));
      expect(plans()).toHaveLength(asked);
    });

    it("say why a channel is transcoded", async () => {
      planAnswer = () => ({ ...hevc, audio_codec: "ac3", audio: "aac" });
      await tuned();
      await toHls();
      open();
      expect(await within(details()).findByText("Transcoding")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Playback details" })).toHaveTextContent("Transcoding");
      expect(within(details()).getByText("This device doesn't play AC3 sound")).toBeInTheDocument();
      expect(within(details()).getByText("HEVC 10-bit · 1920×1080 · 50 fps → H.264 1080p")).toBeInTheDocument();
      expect(within(details()).getByText("5.2 Mbps → 6.0 Mbps")).toBeInTheDocument();
      expect(within(details()).getByText("NVIDIA NVENC")).toBeInTheDocument();
    });

    it("keep the controls on screen while they show", async () => {
      const { container } = await tuned();
      act(() => mpegtsPlayers[0]?.found(itsTracks));
      await waitFor(() => expect(play).toHaveBeenCalled());
      act(() => {
        container.querySelector("video")!.dispatchEvent(new Event("playing"));
      });
      const controls = () => screen.getByRole("button", { name: "Back" }).parentElement;
      await act(() => vi.advanceTimersByTimeAsync(4000));
      expect(controls()).toHaveClass("opacity-0");

      open();
      await act(() => vi.advanceTimersByTimeAsync(6000));
      expect(controls()).not.toHaveClass("opacity-0");
    });
  });
});

describe("LivePlayerPage and the remote", () => {
  const sport: Channel = { ...news, id: 6, name: "Sport", number: "6", group: "Sport" };
  const film: Channel = { ...news, id: 7, name: "Film", number: "7", group: "Film" };

  beforeEach(() => {
    hlsInstances.length = 0;
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => undefined);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  /** Three channels, in the order the page tunes them; each watch gets its own session (5-1, 6-1…). */
  function serveAll(): FakeRequest[] {
    const channels = [news, sport, film];
    const handlers: Parameters<typeof mockApi>[0] = {
      "GET /api/live/browse": () => ({ total: channels.length, offset: 0, channels }),
    };
    for (const c of channels) {
      let n = 0;
      handlers[`GET /api/live/channels/${c.id}`] = () => c;
      handlers[`POST /api/live/channels/${c.id}/watch`] = () => {
        n++;
        return { session_id: `${c.id}-${n}`, url: `/api/live/sessions/${c.id}-${n}/index.m3u8`, kind: "hls", channel: c };
      };
      handlers[`DELETE /api/live/sessions/${c.id}-1`] = () => undefined;
    }
    return mockApi(handlers);
  }

  function Player() {
    return (
      <>
        <PlayerWithEvents />
        <LocationDisplay />
      </>
    );
  }

  const command = (session_id: string, command: "next" | "previous"): ServerEvent => ({
    type: "player_command",
    session_id,
    command,
    position_ms: null,
    offset_ms: null,
    stream_index: null,
  });
  const watched = (requests: FakeRequest[]) =>
    requests.filter((r) => r.method === "POST" && r.path.endsWith("/watch")).map((r) => r.path);
  /** The player's controls are on screen: its top bar, which shows and hides with the rest. */
  const controlsShown = () => !screen.getByRole("button", { name: "Back" }).parentElement!.classList.contains("opacity-0");

  it("says it obeys channel up and down as it watches", async () => {
    const requests = serveAll();
    renderWithProviders(<Player />, { route: "/live/5" });
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    expect(requests.find((r) => r.path === "/api/live/channels/5/watch")?.body).toMatchObject({ controls: ["next", "previous"] });
    expect(isPlayingHere("5-1")).toBe(true);
  });

  it("changes channel for its own session, in the order it tunes them", async () => {
    const requests = serveAll();
    renderWithProviders(<Player />, { route: "/live/5" });
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    await waitFor(() => expect(requests.some((r) => r.path.startsWith("/api/live/browse"))).toBe(true));

    // Another session's: nothing happens here.
    act(() => FakeEventSource.last?.send(command("other", "next")));
    expect(screen.getByTestId("location")).toHaveTextContent("/live/src/5");
    expect(watched(requests)).toHaveLength(1);

    act(() => FakeEventSource.last?.send(command("5-1", "next")));
    await waitFor(() => expect(watched(requests)).toContain("/api/live/channels/6/watch"));
    expect(screen.getByTestId("location")).toHaveTextContent("/live/src/6");
    await waitFor(() => expect(isPlayingHere("6-1")).toBe(true));
    expect(isPlayingHere("5-1")).toBe(false);

    // Down from the first channel: round to the last.
    act(() => FakeEventSource.last?.send(command("6-1", "previous")));
    await waitFor(() => expect(watched(requests).at(-1)).toBe("/api/live/channels/5/watch"));
    await waitFor(() => expect(isPlayingHere("5-2")).toBe(true));
    act(() => FakeEventSource.last?.send(command("5-2", "previous")));
    await waitFor(() => expect(watched(requests).at(-1)).toBe("/api/live/channels/7/watch"));
  });

  it("changes channel for the remote without its controls: the channel's tuning screen shows", async () => {
    // The controls' timer, faked, so they hide as they do after a while.
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"], shouldAdvanceTime: true });
    const requests = serveAll();
    const { container } = renderWithProviders(<Player />, { route: "/live/5" });
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    await waitFor(() => expect(requests.some((r) => r.path.startsWith("/api/live/browse"))).toBe(true));
    const video = container.querySelector("video")!;
    fireEvent.playing(video);
    act(() => {
      vi.advanceTimersByTime(4_000);
    });
    expect(controlsShown()).toBe(false);

    act(() => FakeEventSource.last?.send(command("5-1", "next")));
    expect(await screen.findByLabelText("Tuning")).toBeInTheDocument();
    expect(screen.getByTestId("location")).toHaveTextContent("/live/src/6");
    expect(controlsShown()).toBe(false);
    await waitFor(() => expect(hlsInstances[1]?.calls).toContain("attachMedia"));
    fireEvent.playing(video);
    expect(screen.queryByLabelText("Tuning")).not.toBeInTheDocument();
    expect(controlsShown()).toBe(false);

    // Channel up here: the controls show while it tunes, as they always have.
    fireEvent.keyDown(window, { key: "ArrowUp" });
    expect(controlsShown()).toBe(true);
    await waitFor(() => expect(watched(requests).at(-1)).toBe("/api/live/channels/7/watch"));
    act(() => {
      vi.advanceTimersByTime(4_000);
    });
    expect(controlsShown()).toBe(true);
  });

  it("watches the channel on another device, and leaves it here", async () => {
    const requests = mockApi({
      "GET /api/live/channels/5": () => news,
      "POST /api/live/channels/5/watch": () => ({ session_id: "5-1", url: "/api/live/sessions/5-1/index.m3u8", kind: "hls", channel: news }),
      "DELETE /api/live/sessions/5-1": () => undefined,
      "GET /api/sessions": () => [],
      // A phone without Live TV isn't offered.
      "GET /api/remote/devices": () => [makeReceiver(), makeReceiver({ receiver_id: "phone-1", device_id: "app-3", name: "Phone", casts: ["item"] })],
      "POST /api/remote/devices/tv-1/cast": () => undefined,
    });
    const user = userEvent.setup();
    renderWithProviders(<Player />, { route: "/live/5" });
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));

    await user.click(await screen.findByRole("button", { name: "Watch on…" }));
    expect(screen.getAllByRole("menuitem").map((i) => i.getAttribute("aria-label"))).toEqual(["Living room TV"]);
    await user.click(screen.getByRole("menuitem", { name: "Living room TV" }));
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(/^\/live$/));
    expect(await screen.findByText("Playing on Living room TV")).toBeInTheDocument();
    expect(requests.find((r) => r.path === "/api/remote/devices/tv-1/cast")?.body).toEqual({ channel_id: 5 });
    await waitFor(() => expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/live/sessions/5-1")).toBe(true));
  });

  it("stops when it is stopped from another device, and says so", async () => {
    const requests = serveAll();
    renderWithProviders(<Player />, { route: "/live/5" });
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));

    act(() => FakeEventSource.last?.send({ type: "session_stopped", session_id: "other", message: null, by_admin: false }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    act(() => FakeEventSource.last?.send({ type: "session_stopped", session_id: "5-1", message: "Bedtime", by_admin: false }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Live TV stopped");
    expect(alert).toHaveTextContent("Live TV was stopped from another device. Bedtime");
    expect(hlsInstances[0]?.calls.at(-1)).toBe("destroy");
    expect(isPlayingHere("5-1")).toBe(false);

    // The stream's own 410 that follows says nothing else, and nothing tunes again.
    act(() => hlsInstances[0]?.emit("hlsError", { fatal: true, type: "networkError", response: { code: 410 } }));
    expect(screen.getByRole("alert")).toHaveTextContent("Bedtime");
    expect(watched(requests)).toHaveLength(1);
    // Ended on the server: nothing to leave.
    expect(requests.some((r) => r.method === "DELETE")).toBe(false);
  });
});

describe("LivePlayerPage and picture in picture", () => {
  const sport: Channel = { ...news, id: 6, name: "Sport", number: "6", group: "Sport" };
  /** The browser's media session: the page's handlers, by action, as the window's buttons call them. */
  const actions = new Map<string, (() => void) | null>();

  beforeEach(() => {
    hlsInstances.length = 0;
    actions.clear();
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => undefined);
    Object.defineProperty(navigator, "mediaSession", {
      configurable: true,
      value: { setActionHandler: (action: string, handler: (() => void) | null) => actions.set(action, handler) },
    });
  });
  afterEach(() => {
    Reflect.deleteProperty(navigator, "mediaSession");
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  /** Two channels; each watch gets its own session (5-1, 6-1, 5-2…). */
  function serveTwo(): FakeRequest[] {
    const channels = [news, sport];
    const handlers: Parameters<typeof mockApi>[0] = {
      "GET /api/live/browse": () => ({ total: channels.length, offset: 0, channels }),
    };
    for (const c of channels) {
      let n = 0;
      handlers[`GET /api/live/channels/${c.id}`] = () => c;
      handlers[`POST /api/live/channels/${c.id}/watch`] = () => {
        n++;
        return { session_id: `${c.id}-${n}`, url: `/api/live/sessions/${c.id}-${n}/index.m3u8`, kind: "hls", channel: c };
      };
      handlers[`DELETE /api/live/sessions/${c.id}-1`] = () => undefined;
      handlers[`DELETE /api/live/sessions/${c.id}-2`] = () => undefined;
    }
    return mockApi(handlers);
  }
  const watched = (requests: FakeRequest[]) =>
    requests.filter((r) => r.method === "POST" && r.path.endsWith("/watch")).map((r) => r.path);

  it("lets the browser open its window, as for a video", async () => {
    serveTwo();
    const { container } = renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    expect(container.querySelector("video")).not.toHaveAttribute("disablepictureinpicture");
  });

  it("changes channel from the window's next and previous buttons", async () => {
    const requests = serveTwo();
    const { unmount } = renderWithProviders(<PlayerWithEvents />, { route: "/live/5" });
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    await waitFor(() => expect(requests.some((r) => r.path.startsWith("/api/live/browse"))).toBe(true));

    act(() => actions.get("nexttrack")?.());
    await waitFor(() => expect(watched(requests).at(-1)).toBe("/api/live/channels/6/watch"));
    act(() => actions.get("previoustrack")?.());
    await waitFor(() => expect(watched(requests).at(-1)).toBe("/api/live/channels/5/watch"));
    expect(watched(requests)).toHaveLength(3);

    // Gone with the player: the buttons and media keys are the browser's again.
    unmount();
    expect(actions.get("nexttrack")).toBeNull();
    expect(actions.get("previoustrack")).toBeNull();
  });
});
