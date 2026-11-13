import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useServerEvents } from "@/api/events";
import type { Channel } from "@/api/livetv";
import type { ServerEvent } from "@/api/types";
import { isPlayingHere } from "@/lib/playingHere";
import { makeReceiver } from "@/test/fixtures";
import { LocationDisplay, mockApi, renderWithProviders, testUser, type FakeRequest } from "@/test/render";
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
 */
const { FakeMpegts, mpegtsPlayers, mpegtsLoader } = vi.hoisted(() => {
  const mpegtsPlayers: FakeMpegtsPlayer[] = [];
  class FakeMpegtsPlayer {
    static failsAsItLoads = true;
    readonly calls: string[] = [];
    private readonly handlers = new Map<string, () => void>();
    private engine: object | null = {};
    constructor() {
      mpegtsPlayers.push(this);
    }
    on(event: string, handler: () => void) {
      this.handlers.set(event, handler);
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
    Events: { ERROR: "error" },
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
