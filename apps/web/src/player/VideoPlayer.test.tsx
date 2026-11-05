import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useServerEvents } from "@/api/events";
import type { ItemDetail, PlaybackInfo, ServerEvent } from "@/api/types";
import { makeItem, makeStream } from "@/test/fixtures";
import { pgsMks } from "@/test/pictures";
import { LocationDisplay, mockApi, renderWithProviders, testUser, type FakeRequest } from "@/test/render";
import { isPlayingHere } from "@/lib/playingHere";
import { forgetNetwork } from "./network";
import { VideoPlayer, startSize } from "./VideoPlayer";

/** A stand-in for hls.js: records its config and calls, and fires events on demand. */
const { FakeHls, hlsInstances } = vi.hoisted(() => {
  type Handler = (event: string, data: unknown) => void;
  const hlsInstances: FakeHls[] = [];
  class FakeHls {
    static isSupported = () => true;
    static Events = {
      ERROR: "hlsError",
      FRAG_LOADED: "hlsFragLoaded",
      FRAG_PARSED: "hlsFragParsed",
      INIT_PTS_FOUND: "hlsInitPtsFound",
      MANIFEST_PARSED: "hlsManifestParsed",
      LEVEL_SWITCHED: "hlsLevelSwitched",
    };
    startLevel = -1;
    levels: { width: number; height: number }[] = [];
    static ErrorTypes = { NETWORK_ERROR: "networkError", MEDIA_ERROR: "mediaError" };
    static ErrorDetails = {
      MANIFEST_LOAD_ERROR: "manifestLoadError",
      MANIFEST_LOAD_TIMEOUT: "manifestLoadTimeOut",
      MANIFEST_PARSING_ERROR: "manifestParsingError",
    };
    readonly calls: string[] = [];
    private readonly handlers = new Map<string, Handler[]>();
    readonly config: Record<string, unknown>;
    constructor(config: Record<string, unknown>) {
      this.config = config;
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

vi.mock("@/lib/hls", () => ({ loadHls: () => Promise.resolve(FakeHls) }));

function detail(overrides: Partial<ItemDetail> = {}): ItemDetail {
  return {
    ...makeItem({ id: 7, kind: "movie", title: "Heat", duration_ms: 6_000_000 }),
    original_title: null,
    tagline: null,
    genres: [],
    studio: null,
    content_rating: null,
    tmdb_id: null,
    imdb_id: null,
    files: [
      {
        id: 3,
        path: "/movies/Heat.mkv",
        size: 1,
        container: "mkv",
        duration_ms: 6_000_000,
        bitrate: null,
        width: 1920,
        height: 1080,
        video_codec: "h264",
        audio_codec: "aac",
        streams: [],
        edition: null,
        selected_audio: null,
        selected_subtitle: null,
        chapters: [],
      },
    ],
    ...overrides,
  };
}

function session(id: string, overrides: Partial<PlaybackInfo> = {}): PlaybackInfo {
  return {
    session_id: id,
    method: "transcode",
    url: `/api/playback/${id}/index.m3u8`,
    hls: true,
    file_id: 3,
    duration_ms: 6_000_000,
    audio_index: null,
    subtitle_index: null,
    burn_subtitles: false,
    subtitles: [],
    reason: null,
    ...overrides,
  };
}

/** The server: each POST /api/playback answers the next session in `sessions`. */
function serve(sessions: PlaybackInfo[], more: Parameters<typeof mockApi>[0] = {}): FakeRequest[] {
  let next = 0;
  return mockApi({
    "POST /api/playback": () => sessions[next++] ?? session(`extra-${next}`),
    "POST /api/items/7/progress": () => undefined,
    ...more,
  });
}

function renderPlayer(startMs = 0) {
  return renderWithProviders(<VideoPlayer item={detail()} startMs={startMs} fileId={null} audio={null} sub={null} />);
}

/** The media element methods the player calls, which jsdom only stubs. */
const media = {
  play: vi.fn<() => Promise<void>>(),
  pause: vi.fn<() => void>(),
  load: vi.fn<() => void>(),
};

beforeEach(() => {
  hlsInstances.length = 0;
  // jsdom has media elements, but plays nothing.
  media.play = vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
  media.pause = vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
  media.load = vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => undefined);
  vi.spyOn(HTMLMediaElement.prototype, "addTextTrack").mockImplementation(
    () =>
      ({
        cues: null,
        mode: "disabled",
        addCue: () => undefined,
        removeCue: () => undefined,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
      }) as unknown as TextTrack,
  );
});

describe("VideoPlayer leaving", () => {
  it("lets go of a direct play's file, so its download doesn't compete with what plays next", async () => {
    const requests = serve([session("d1", { method: "direct", hls: false, url: "/api/files/3/stream" })], {
      "DELETE /api/playback/d1": () => undefined,
    });
    const { container, unmount } = renderPlayer(0);
    const video = container.querySelector("video");
    if (!video) throw new Error("no video element");
    await waitFor(() => expect(video.getAttribute("src")).toBe("/api/files/3/stream"));
    media.pause.mockClear();
    media.load.mockClear();

    unmount();

    expect(video.hasAttribute("src")).toBe(false);
    expect(media.pause).toHaveBeenCalled();
    expect(media.load).toHaveBeenCalled();
    // And the session ends, as before.
    await waitFor(() => expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/playback/d1")).toBe(true));
  });

  it("lets go of hls.js", async () => {
    serve([session("s1")], { "DELETE /api/playback/s1": () => undefined });
    const { unmount } = renderPlayer(0);
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    unmount();
    expect(hlsInstances[0]?.calls.at(-1)).toBe("destroy");
  });
});

describe("VideoPlayer switching sessions", () => {
  const starts = (requests: FakeRequest[]) => requests.filter((r) => r.method === "POST" && r.path === "/api/playback");

  it("asks for the next session without waiting for the old one's release", async () => {
    // The old session's last report and release never get an answer.
    const requests = serve([session("s1"), session("s2")], {
      "POST /api/items/7/progress": () => new Promise(() => undefined),
      "DELETE /api/playback/s1": () => new Promise(() => undefined),
    });
    renderPlayer(0);
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));

    // The session expired: the player starts a new one.
    act(() => {
      hlsInstances[0]?.emit("hlsError", { fatal: true, type: "networkError", details: "fragLoadError", response: { code: 404 } });
    });

    await waitFor(() => expect(hlsInstances).toHaveLength(2));
    expect(starts(requests)).toHaveLength(2);
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/playback/s1")).toBe(true);
    expect(requests.some((r) => r.path === "/api/items/7/progress" && (r.body as { state: string }).state === "stopped")).toBe(true);
  });

  it("says the server can't be reached when its stream stops answering", async () => {
    serve([session("s1")]);
    renderPlayer(0);
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));

    // No answer at all, past the retries.
    act(() => {
      for (let i = 0; i < 5; i++) hlsInstances[0]?.emit("hlsError", { fatal: true, type: "networkError", details: "fragLoadError" });
    });

    expect(await screen.findByText("Can't reach your server")).toBeInTheDocument();
    expect(screen.getByText("Check that the server is on and that this device is on its network.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  it("still waits for a start in flight, which ends its own session", async () => {
    let answer: (info: PlaybackInfo) => void = () => undefined;
    const first = new Promise<PlaybackInfo>((resolve) => (answer = resolve));
    let posts = 0;
    const requests = mockApi({
      "POST /api/playback": () => (posts++ === 0 ? first : session("s2")),
      "DELETE /api/playback/s1": () => undefined,
    });
    renderPlayer(0);
    await waitFor(() => expect(starts(requests)).toHaveLength(1));

    // Back from the back/forward cache before the first start answered.
    act(() => {
      window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true }));
    });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(starts(requests)).toHaveLength(1);

    answer(session("s1"));
    await waitFor(() => expect(starts(requests)).toHaveLength(2));
    const paths = requests.map((r) => `${r.method} ${r.path}`);
    expect(paths.indexOf("DELETE /api/playback/s1")).toBeGreaterThanOrEqual(0);
    expect(paths.indexOf("DELETE /api/playback/s1")).toBeLessThan(paths.lastIndexOf("POST /api/playback"));
    await waitFor(() => expect(hlsInstances.map((h) => h.calls[0])).toEqual(["loadSource /api/playback/s2/index.m3u8"]));
  });
});

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

/** The player with the app's event stream open, as signed in. */
function PlayerWithEvents() {
  useServerEvents(testUser.id);
  return <VideoPlayer item={detail()} startMs={0} fileId={null} audio={null} sub={null} />;
}

describe("VideoPlayer stopped on the server", () => {
  beforeEach(() => {
    vi.stubGlobal("EventSource", FakeEventSource);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const starts = (requests: FakeRequest[]) => requests.filter((r) => r.method === "POST" && r.path === "/api/playback");

  it("stops, and says who stopped it and why", async () => {
    const requests = serve([session("s1")], { "DELETE /api/playback/s1": () => undefined });
    const { unmount } = renderWithProviders(<PlayerWithEvents />);
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));

    // Someone else's session: nothing happens here.
    act(() => FakeEventSource.last?.send({ type: "session_stopped", session_id: "other", message: null, by_admin: true }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    act(() => FakeEventSource.last?.send({ type: "session_stopped", session_id: "s1", message: "Server restarting", by_admin: true }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Playback was stopped by the server's administrator");
    expect(alert).toHaveTextContent("Server restarting");
    expect(hlsInstances[0]?.calls.at(-1)).toBe("destroy");

    unmount();
    // Nothing starts again, and there is nothing left to release.
    expect(starts(requests)).toHaveLength(1);
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/playback/s1")).toBe(false);
  });

  it("stops when the stream is gone (410), with the server's message", async () => {
    const requests = serve([session("s1")], {
      "GET /api/playback/s1/index.m3u8": () =>
        new Response(JSON.stringify({ error: "session_stopped", message: "Too many streams" }), {
          status: 410,
          headers: { "content-type": "application/json" },
        }),
    });
    renderWithProviders(<PlayerWithEvents />);
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));

    act(() => {
      hlsInstances[0]?.emit("hlsError", { fatal: true, type: "networkError", details: "fragLoadError", response: { code: 410 } });
    });

    const alert = await screen.findByRole("alert");
    await waitFor(() => expect(alert).toHaveTextContent("Too many streams"));
    expect(alert).toHaveTextContent("Playback was stopped by the server's administrator");
    expect(starts(requests)).toHaveLength(1);
  });

  it("says so when it was stopped from another of the user's devices", async () => {
    serve([session("s1")]);
    renderWithProviders(<PlayerWithEvents />);
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    act(() => FakeEventSource.last?.send({ type: "session_stopped", session_id: "s1", message: null, by_admin: false }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Playback was stopped from another device");
  });
});

/** Stands in for the browser's VTTCue. */
class FakeCue {
  line: number | "auto" = "auto";
  snapToLines = true;
  startTime: number;
  endTime: number;
  text: string;
  constructor(startTime: number, endTime: number, text: string) {
    this.startTime = startTime;
    this.endTime = endTime;
    this.text = text;
  }
  getCueAsHTML() {
    const fragment = document.createDocumentFragment();
    fragment.append(this.text);
    return fragment;
  }
}

describe("VideoPlayer subtitle timing", () => {
  beforeEach(() => {
    vi.stubGlobal("VTTCue", FakeCue);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const english = makeStream({ index: 3, kind: "subtitle", codec: "subrip", language: "eng", default: false, text_based: true });

  it("shows the lines moved by the offset remembered for the file, until it is reset", async () => {
    const user = userEvent.setup();
    window.localStorage.setItem("pley.subtitleOffsets", JSON.stringify({ f3: 1 }));
    serve(
      [
        session("s1", {
          subtitles: [
            { index: 3, codec: "subrip", language: "eng", title: null, default: false, forced: false, external: false, text_based: true, url: "/api/subs/3.vtt" },
          ],
        }),
      ],
      { "GET /api/subs/3.vtt": () => new Response("WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHello", { status: 200 }) },
    );
    const item = detail();
    const file = item.files[0];
    if (!file) throw new Error("no file");
    file.streams = [english];
    const { container } = renderWithProviders(<VideoPlayer item={item} startMs={0} fileId={null} audio={null} sub={3} />);
    const video = container.querySelector("video");
    if (!video) throw new Error("no video element");
    Object.defineProperty(video, "currentTime", { value: 2.5, configurable: true, writable: true });

    // At 2.5 s, one second later than the file has it.
    await waitFor(() => {
      fireEvent.timeUpdate(video);
      expect(screen.getByTestId("subtitles")).toHaveTextContent("Hello");
    });

    await user.click(screen.getByRole("button", { name: "Playback settings" }));
    await user.click(screen.getByRole("button", { name: /Subtitles.*\+1\.0 s/ }));
    await user.click(screen.getByRole("button", { name: "Reset" }));
    fireEvent.timeUpdate(video);
    expect(screen.queryByTestId("subtitles")).not.toBeInTheDocument();
    expect(window.localStorage.getItem("pley.subtitleOffsets")).toBe("{}");
  });

  it("remembers subtitles turned on, for the next episode and for this one played again", async () => {
    const requests = serve([session("s1")], {
      "PUT /api/items/7/streams": () => undefined,
      "GET /api/items/8": () => detail({ id: 8 }),
    });
    const item = detail();
    const file = item.files[0];
    if (!file) throw new Error("no file");
    file.streams = [english];
    const { client } = renderWithProviders(<VideoPlayer item={item} startMs={0} fileId={null} audio={null} sub={null} />);
    client.setQueryData(["item", 7], item);
    client.setQueryData(["item", 8], detail({ id: 8 }));
    await waitFor(() => expect(requests.some((r) => r.path === "/api/playback")).toBe(true));

    fireEvent.keyDown(document, { key: "c" });
    await waitFor(() =>
      // The audio left out: a subtitle change keeps what was picked before.
      expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ file_id: 3, subtitle_index: 3 }),
    );
    // The next one is asked again; this one once it shows again, not while it plays.
    await waitFor(() => expect(client.getQueryState(["item", 7])?.isInvalidated).toBe(true));
    expect(client.getQueryState(["item", 8])?.isInvalidated || requests.some((r) => r.path === "/api/items/8")).toBe(true);
    expect(requests.some((r) => r.path === "/api/items/7")).toBe(false);
  });

  it("moves the track's own cues in native fullscreen, where the browser draws them", async () => {
    window.localStorage.setItem("pley.subtitleOffsets", JSON.stringify({ f3: 1 }));
    const onTrack: FakeCue[] = [];
    const track = {
      get cues() {
        return onTrack;
      },
      mode: "disabled",
      addCue: (cue: FakeCue) => onTrack.push(cue),
      removeCue: (cue: FakeCue) => onTrack.splice(onTrack.indexOf(cue), 1),
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    };
    vi.spyOn(HTMLMediaElement.prototype, "addTextTrack").mockImplementation(() => track as unknown as TextTrack);
    serve(
      [
        session("s1", {
          subtitles: [
            { index: 3, codec: "subrip", language: "eng", title: null, default: false, forced: false, external: false, text_based: true, url: "/api/subs/3.vtt" },
          ],
        }),
      ],
      { "GET /api/subs/3.vtt": () => new Response("WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHello", { status: 200 }) },
    );
    const item = detail();
    const file = item.files[0];
    if (!file) throw new Error("no file");
    file.streams = [english];
    const { container } = renderWithProviders(<VideoPlayer item={item} startMs={0} fileId={null} audio={null} sub={3} />);
    const video = container.querySelector("video");
    if (!video) throw new Error("no video element");

    // Drawn by pley: the cues stay at the file's times, hidden.
    await waitFor(() => expect(onTrack.map((c) => [c.startTime, c.endTime])).toEqual([[1, 2]]));
    expect(track.mode).toBe("hidden");

    // Drawn by the browser: moved one second later on the track.
    fireEvent(video, new Event("webkitbeginfullscreen"));
    await waitFor(() => expect(onTrack.map((c) => [c.startTime, c.endTime])).toEqual([[2, 3]]));
    expect(track.mode).toBe("showing");
    expect(onTrack[0]?.text).toBe("Hello");
  });
});

describe("VideoPlayer audio", () => {
  /** English TrueHD (the file's default) and AAC, English subtitles; the remembered choice the AAC. */
  function withTracks(): ItemDetail {
    const item = detail();
    const file = item.files[0];
    if (!file) throw new Error("no file");
    file.streams = [
      makeStream({ index: 0, kind: "video" }),
      makeStream({ index: 1, kind: "audio", codec: "truehd", language: "eng", default: true }),
      makeStream({ index: 2, kind: "audio", codec: "aac", language: "eng", default: false }),
      makeStream({ index: 3, kind: "subtitle", codec: "subrip", language: "eng", default: false, text_based: true }),
    ];
    file.selected_audio = 2;
    return item;
  }
  const put = (requests: FakeRequest[]) =>
    requests.filter((r) => r.method === "PUT" && r.path === "/api/items/7/streams").map((r) => r.body as Record<string, unknown>);

  it("leaves the audio to the server, which plays what this browser plays best, and saves subtitles alone without it", async () => {
    const requests = serve([session("s1", { audio_index: 2 })], { "PUT /api/items/7/streams": () => undefined });
    renderWithProviders(<VideoPlayer item={withTracks()} startMs={0} fileId={null} audio={null} sub={null} />);
    await waitFor(() => expect(hlsInstances).toHaveLength(1));
    const started = requests.find((r) => r.method === "POST" && r.path === "/api/playback")?.body as Record<string, unknown>;
    expect(started.audio_index).toBeNull();

    fireEvent.keyDown(document, { key: "c" });
    await waitFor(() => expect(put(requests)).toHaveLength(1));
    expect(put(requests)[0]).toEqual({ file_id: 3, subtitle_index: 3 });
  });

  it("plays and keeps a track picked", async () => {
    const requests = serve([session("s1", { audio_index: 1 })], { "PUT /api/items/7/streams": () => undefined });
    renderWithProviders(<VideoPlayer item={withTracks()} startMs={0} fileId={null} audio={1} sub={null} />);
    await waitFor(() => expect(hlsInstances).toHaveLength(1));
    expect((requests.find((r) => r.method === "POST" && r.path === "/api/playback")?.body as Record<string, unknown>).audio_index).toBe(1);

    fireEvent.keyDown(document, { key: "c" });
    await waitFor(() => expect(put(requests)).toHaveLength(1));
    expect(put(requests)[0]).toEqual({ file_id: 3, audio_index: 1, subtitle_index: 3 });
  });
});

describe("VideoPlayer picture subtitles", () => {
  const pgs = (index: number, language: string) =>
    makeStream({ index, kind: "subtitle", codec: "hdmv_pgs_subtitle", language, default: false, text_based: false });
  /** An MKV with English and French PGS subtitles. */
  function withPictures(): ItemDetail {
    const item = detail();
    const file = item.files[0];
    if (!file) throw new Error("no file");
    file.streams = [makeStream({ index: 0, kind: "video" }), makeStream({ index: 1, kind: "audio", codec: "aac" }), pgs(4, "eng"), pgs(5, "fre")];
    return item;
  }
  const starts = (requests: FakeRequest[]) =>
    requests.filter((r) => r.method === "POST" && r.path === "/api/playback").map((r) => r.body as Record<string, unknown>);
  const mksAnswer = () => new Response(pgsMks([[1000, 2000]]), { status: 200, headers: { "content-type": "video/x-matroska" } });
  const direct = (id: string) => session(id, { method: "direct", hls: false, url: "/api/files/3/stream" });

  it("draws them over the video from their own file: the server is asked for no subtitles, and plays the file as it is", async () => {
    const requests = serve([direct("d1")], { "GET /api/files/3/subtitles/4.mks": mksAnswer });
    const { container } = renderWithProviders(<VideoPlayer item={withPictures()} startMs={0} fileId={null} audio={null} sub={4} />);
    await waitFor(() => expect(container.querySelector("video")?.getAttribute("src")).toBe("/api/files/3/stream"));
    expect(starts(requests)[0]?.subtitle_index).toBeNull();
    expect(starts(requests)[0]?.caps).toMatchObject({ picture_subtitles_sideload: true });
    await waitFor(() => expect(requests.some((r) => r.path.startsWith("/api/files/3/subtitles/4.mks"))).toBe(true));
    expect(screen.getByTestId("picture-subtitles")).toBeInTheDocument();
  });

  it("changes picture tracks without a new session, and moves them as text", async () => {
    const user = userEvent.setup();
    const requests = serve([direct("d1")], {
      "GET /api/files/3/subtitles/4.mks": mksAnswer,
      "GET /api/files/3/subtitles/5.mks": mksAnswer,
      "PUT /api/items/7/streams": () => undefined,
    });
    renderWithProviders(<VideoPlayer item={withPictures()} startMs={0} fileId={null} audio={null} sub={4} />);
    await waitFor(() => expect(requests.some((r) => r.path.startsWith("/api/files/3/subtitles/4.mks"))).toBe(true));

    await user.click(screen.getByRole("button", { name: "Playback settings" }));
    await user.click(screen.getByRole("button", { name: /^Subtitles/ }));
    expect(screen.getByRole("button", { name: "Show subtitles earlier" })).toBeInTheDocument();
    await user.click(screen.getByRole("menuitemradio", { name: /French/ }));

    await waitFor(() => expect(requests.some((r) => r.path.startsWith("/api/files/3/subtitles/5.mks"))).toBe(true));
    expect(starts(requests)).toHaveLength(1);
  });

  it("has them burned in, as before, by a server that can't make their file", async () => {
    let answer404: () => void = () => undefined;
    const requests = serve([direct("d1"), session("s2", { burn_subtitles: true, subtitle_index: 4 })], {
      "GET /api/files/3/subtitles/4.mks": () =>
        new Promise<Response>((resolve) => (answer404 = () => resolve(new Response(JSON.stringify({ code: "not_found" }), { status: 404 })))),
    });
    const { container } = renderWithProviders(<VideoPlayer item={withPictures()} startMs={0} fileId={null} audio={null} sub={4} />);
    await waitFor(() => expect(container.querySelector("video")?.getAttribute("src")).toBe("/api/files/3/stream"));
    expect(starts(requests)[0]?.subtitle_index).toBeNull();

    act(() => answer404());

    await waitFor(() => expect(starts(requests)).toHaveLength(2));
    expect(starts(requests)[1]?.subtitle_index).toBe(4);
    expect(screen.queryByTestId("picture-subtitles")).not.toBeInTheDocument();
    // Asked once: other picture tracks are burned in too, without asking.
    expect(requests.filter((r) => r.path.endsWith(".mks"))).toHaveLength(1);
  });

  it("keeps the iPhone's own full screen, where nothing drawn over the video shows, for when none are drawn", async () => {
    const enter = vi.fn();
    const before = Object.getOwnPropertyDescriptor(document.documentElement, "requestFullscreen");
    Object.defineProperty(document.documentElement, "requestFullscreen", { value: undefined, configurable: true });
    Object.defineProperty(HTMLVideoElement.prototype, "webkitEnterFullscreen", { value: enter, configurable: true });
    try {
      serve([direct("d1")], { "GET /api/files/3/subtitles/4.mks": mksAnswer });
      renderWithProviders(<VideoPlayer item={withPictures()} startMs={0} fileId={null} audio={null} sub={4} />);
      await waitFor(() => expect(screen.getByTestId("picture-subtitles")).toBeInTheDocument());
      expect(screen.queryByRole("button", { name: "Fullscreen" })).not.toBeInTheDocument();
      fireEvent.keyDown(document, { key: "f" });
      expect(enter).not.toHaveBeenCalled();
    } finally {
      Reflect.deleteProperty(HTMLVideoElement.prototype, "webkitEnterFullscreen");
      if (before) Object.defineProperty(document.documentElement, "requestFullscreen", before);
      else Reflect.deleteProperty(document.documentElement, "requestFullscreen");
    }
  });
});

describe("VideoPlayer with hls.js", () => {
  it("starts just past 0 from the beginning, so hls.js seeks to the first frame", async () => {
    serve([session("s1")]);
    renderPlayer(0);
    await waitFor(() => expect(hlsInstances).toHaveLength(1));
    expect(hlsInstances[0]?.config.startPosition).toBe(0.001);
  });

  it("starts at the resume point when there is one", async () => {
    serve([session("s1")]);
    renderPlayer(90_000);
    await waitFor(() => expect(hlsInstances).toHaveLength(1));
    expect(hlsInstances[0]?.config.startPosition).toBe(90);
  });

  it("asks for the first segment without waiting for the video element", async () => {
    serve([session("s1")]);
    renderPlayer(0);
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    expect(hlsInstances[0]?.config.startFragPrefetch).toBe(true);
  });

  it("starts at the size the server lists first, once the playlist is read", async () => {
    serve([session("s1")]);
    renderPlayer(90_000);
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    const hls = hlsInstances[0]!;
    // Not at hls.js's guess from a bandwidth it hasn't measured.
    expect(hls.config.autoStartLoad).toBe(false);
    expect(hls.calls).not.toContain("startLoad");
    const levels = [
      { width: 640, height: 360 },
      { width: 1280, height: 720 },
      { width: 1920, height: 1080 },
    ];
    act(() => hls.emit("hlsManifestParsed", { levels, firstLevel: 2 }));
    expect(hls.startLevel).toBe(2);
    expect(hls.calls).toContain("startLoad");
  });
});

describe("startSize", () => {
  const levels = [
    { width: 640, height: 360 },
    { width: 1280, height: 720 },
    { width: 1920, height: 1080 },
  ];
  it("is the smallest that fills the player, never above the server's first", () => {
    expect(startSize(levels, { clientWidth: 1100, clientHeight: 620 }, 2)).toBe(1);
    expect(startSize(levels, { clientWidth: 2560, clientHeight: 1440 }, 2)).toBe(2);
    expect(startSize(levels, { clientWidth: 1100, clientHeight: 620 }, 0)).toBe(0);
    expect(startSize(levels, { clientWidth: 0, clientHeight: 0 }, 2), "not laid out yet").toBe(2);
  });
  it("starts a size listed twice at its higher bitrate", () => {
    // 20 Mbps 1080p, also at 1080p's 8 Mbps.
    const twice = [...levels, { width: 1920, height: 1080 }];
    expect(startSize(twice, { clientWidth: 1600, clientHeight: 900 }, 3)).toBe(3);
    expect(startSize(twice, { clientWidth: 1100, clientHeight: 620 }, 3)).toBe(1);
  });
});

describe("VideoPlayer in Automatic", () => {
  const starts = (requests: FakeRequest[]) =>
    requests.filter((r) => r.method === "POST" && r.path === "/api/playback").map((r) => r.body as Record<string, unknown>);

  beforeEach(() => forgetNetwork());

  /** A 20 Mbps 1080p file, played as it is (`remux`: into HLS, its audio converted); `more`: other answers of the server's. */
  function render20(more: Parameters<typeof mockApi>[0] = {}, remux = false) {
    const item = detail();
    item.files[0]!.bitrate = 20_000_000;
    const first = remux ? session("r1", { method: "remux" }) : session("d1", { method: "direct", hls: false, url: "/api/files/3/stream" });
    const requests = serve([first, session("t2")], {
      "DELETE /api/playback/d1": () => undefined,
      "DELETE /api/playback/r1": () => undefined,
      ...more,
    });
    const view = renderWithProviders(<VideoPlayer item={item} startMs={60_000} fileId={null} audio={null} sub={null} />);
    return { requests, ...view };
  }

  /** Two segments of the remux came at `mbps` (from their first byte). */
  function segments(mbps: number) {
    act(() => {
      for (let i = 0; i < 2; i++) hlsInstances[0]!.emit("hlsFragLoaded", { frag: { stats: { loaded: mbps * 125_000, loading: { start: 0, first: 0, end: 1_000 } } } });
    });
  }

  /** The element plays at 60 s with `ahead` seconds loaded: what jsdom's element can't say by itself. */
  function playing(video: HTMLVideoElement, ahead = 5) {
    const state = { time: 60, paused: false, seeking: false };
    Object.defineProperty(video, "readyState", { configurable: true, get: () => 4 });
    Object.defineProperty(video, "paused", { configurable: true, get: () => state.paused });
    Object.defineProperty(video, "seeking", { configurable: true, get: () => state.seeking });
    Object.defineProperty(video, "currentTime", { configurable: true, get: () => state.time, set: (t: number) => (state.time = t) });
    Object.defineProperty(video, "buffered", {
      configurable: true,
      get: () => ({ length: 1, start: () => 0, end: () => state.time + ahead }),
    });
    act(() => {
      video.dispatchEvent(new Event("timeupdate"));
      video.dispatchEvent(new Event("playing"));
    });
    return state;
  }

  const sleep = (ms: number) => act(() => new Promise((resolve) => setTimeout(resolve, ms)));

  it("plays the original, and the menu says so", async () => {
    const { requests } = render20();
    await waitFor(() => expect(starts(requests)).toHaveLength(1));
    expect(starts(requests)[0]).toMatchObject({ mode: "auto", max_bitrate: null });
    await userEvent.click(screen.getByRole("button", { name: "Playback settings" }));
    expect(screen.getByRole("button", { name: /Quality\s*Automatic · Original/ })).toBeInTheDocument();
  });

  it("steps down on a 2 s wait for data on a network too slow for the file, where it was, and says so once", async () => {
    const { requests, container } = render20({}, true);
    const video = container.querySelector("video")!;
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    playing(video, 0);
    segments(16);
    act(() => {
      video.dispatchEvent(new Event("waiting"));
    });
    // 20 Mbps × 16/20 (the network's) × 0.75 = 12 → 12 Mbps 1080p.
    await waitFor(() => expect(starts(requests)).toHaveLength(2), { timeout: 4_000 });
    expect(starts(requests)[1]).toMatchObject({ mode: "transcode", max_bitrate: 12_000, max_height: 1080, start_ms: 60_000 });
    expect(await screen.findByText("Quality lowered to 12 Mbps 1080p")).toBeInTheDocument();
    expect(screen.getAllByText("Quality lowered to 12 Mbps 1080p")).toHaveLength(1);
    expect(JSON.parse(window.localStorage.getItem("pley.autoQuality")!)).toMatchObject({ kbps: 12_000 });
    // Nothing was picked: the next title starts in Automatic too.
    expect(window.localStorage.getItem("pley.quality")).toBeNull();
  });

  it("plays the original on through a wait for data on a network that carries it: the server is why", async () => {
    const { requests, container } = render20({}, true);
    const video = container.querySelector("video")!;
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    playing(video, 0);
    segments(40);
    act(() => {
      video.dispatchEvent(new Event("waiting"));
    });
    await sleep(3_100);
    expect(starts(requests)).toHaveLength(1);
  }, 15_000);

  it("doesn't count a seek's wait, nor a wait while paused", async () => {
    const { requests, container } = render20();
    const video = container.querySelector("video")!;
    await waitFor(() => expect(video.getAttribute("src")).toBe("/api/files/3/stream"));
    const state = playing(video);
    state.seeking = true;
    act(() => {
      video.dispatchEvent(new Event("seeking"));
      video.dispatchEvent(new Event("waiting"));
    });
    state.seeking = false;
    await sleep(3_100);
    state.paused = true;
    act(() => {
      video.dispatchEvent(new Event("pause"));
      video.dispatchEvent(new Event("waiting"));
    });
    await sleep(3_100);
    expect(starts(requests)).toHaveLength(1);
  }, 15_000);

  it("keeps a quality picked before, Original included: a slow network waits", async () => {
    window.localStorage.setItem("pley.quality", JSON.stringify("original"));
    const { requests, container } = render20();
    const video = container.querySelector("video")!;
    await waitFor(() => expect(video.getAttribute("src")).toBe("/api/files/3/stream"));
    playing(video);
    act(() => {
      video.dispatchEvent(new Event("waiting"));
    });
    await sleep(3_100);
    expect(starts(requests)).toHaveLength(1);
  }, 15_000);

  it("starts at the ceiling the server knows for this network, which it asks first, over the browser's own", async () => {
    window.localStorage.setItem("pley.autoQuality", JSON.stringify({ kbps: 3_000, at: Date.now() - 60_000 }));
    const { requests } = render20({
      "GET /api/playback/network": () => ({ local: false, ceiling_kbps: 9_000, source: "playback", since: Date.now() - 3_600_000, probe: false }),
    });
    await waitFor(() => expect(starts(requests)).toHaveLength(1));
    const asked = requests.findIndex((r) => r.path.startsWith("/api/playback/network?client_id=web-"));
    expect(asked).toBeGreaterThanOrEqual(0);
    expect(asked).toBeLessThan(requests.findIndex((r) => r.method === "POST" && r.path === "/api/playback"));
    expect(starts(requests)[0]).toMatchObject({ mode: "transcode", max_bitrate: 8_000, max_height: 1080 });
    await userEvent.click(screen.getByRole("button", { name: "Playback settings" }));
    await userEvent.click(screen.getByRole("button", { name: /Quality/ }));
    expect(screen.getByRole("menuitemradio", { name: /Automatic\s*Now 8 Mbps 1080p · Lowered for your connection/ })).toBeInTheDocument();
  });

  it("starts at the original when the server knows of no limit, whatever the browser remembers", async () => {
    window.localStorage.setItem("pley.autoQuality", JSON.stringify({ kbps: 3_000, at: Date.now() - 60_000 }));
    const { requests } = render20({
      "GET /api/playback/network": () => ({ local: true, ceiling_kbps: null, source: null, since: null, probe: false }),
    });
    await waitFor(() => expect(starts(requests)).toHaveLength(1));
    expect(starts(requests)[0]).toMatchObject({ mode: "auto", max_bitrate: null });
  });

  it("tells the server the ceiling of a step down, and to forget it when Automatic is picked again", async () => {
    const nothing = { local: false, ceiling_kbps: null, source: null, since: null, probe: false };
    const { requests, container } = render20({ "GET /api/playback/network": () => nothing, "PUT /api/playback/network": () => nothing }, true);
    const video = container.querySelector("video")!;
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    playing(video, 0);
    segments(16);
    act(() => {
      video.dispatchEvent(new Event("waiting"));
    });
    await waitFor(() => expect(starts(requests)).toHaveLength(2), { timeout: 4_000 });
    const puts = () => requests.filter((r) => r.method === "PUT" && r.path === "/api/playback/network").map((r) => r.body);
    await waitFor(() => expect(puts()).toEqual([{ client_id: expect.stringMatching(/^web-/) as unknown, ceiling_kbps: 12_000 }]));

    await userEvent.click(screen.getByRole("button", { name: "Playback settings" }));
    await userEvent.click(screen.getByRole("button", { name: /Quality/ }));
    await userEvent.click(screen.getByRole("menuitemradio", { name: /Automatic/ }));
    await waitFor(() => expect(puts()).toHaveLength(2));
    expect(puts()[1]).toMatchObject({ ceiling_kbps: null });
    await waitFor(() => expect(starts(requests)).toHaveLength(3));
    expect(starts(requests)[2]).toMatchObject({ mode: "auto", max_bitrate: null });
  }, 15_000);

  it("starts the next title at the network it remembers", async () => {
    window.localStorage.setItem("pley.autoQuality", JSON.stringify({ kbps: 9_000, at: Date.now() - 60_000 }));
    const { requests } = render20();
    await waitFor(() => expect(starts(requests)).toHaveLength(1));
    expect(starts(requests)[0]).toMatchObject({ mode: "transcode", max_bitrate: 8_000, max_height: 1080 });
  });
});

describe("VideoPlayer speed", () => {
  it("plays at the speed picked, also once a new session starts, and > and < step it", async () => {
    serve([session("s1"), session("s2")], { "DELETE /api/playback/s1": () => undefined });
    window.localStorage.setItem("pley.quality", JSON.stringify("original"));
    const { container } = renderPlayer(0);
    const video = container.querySelector("video")!;
    await waitFor(() => expect(hlsInstances).toHaveLength(1));
    await userEvent.click(screen.getByRole("button", { name: "Playback settings" }));
    await userEvent.click(screen.getByRole("button", { name: /Speed\s*Normal/ }));
    await userEvent.click(screen.getByRole("menuitemradio", { name: "1.5×" }));
    expect([video.playbackRate, video.defaultPlaybackRate]).toEqual([1.5, 1.5]);
    fireEvent.keyDown(document.body, { key: ">" });
    expect(video.playbackRate).toBe(2);
    expect(await screen.findByText("Speed: 2×")).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "<" });
    fireEvent.keyDown(document.body, { key: "<" });
    expect(video.playbackRate).toBe(1.25);
    window.localStorage.removeItem("pley.quality");
  });
});

describe("VideoPlayer finding subtitles", () => {
  beforeEach(() => vi.stubGlobal("VTTCue", FakeCue));
  afterEach(() => vi.unstubAllGlobals());

  it("shows subtitles found at once, chosen, and lets whoever added them delete them", async () => {
    const user = userEvent.setup();
    const track = {
      index: 2000,
      codec: "subrip",
      language: "eng",
      title: null,
      default: false,
      forced: false,
      external: true,
      text_based: true,
      url: "/api/files/3/subtitles/2000.vtt",
      added: true,
    };
    const requests = serve([session("s1")], {
      "GET /api/items/7/subtitles/search": () => ({
        results: [{ id: "opensubtitles:9", provider: "opensubtitles", language: "en", name: "Heat.1995", format: "srt", hearing_impaired: false, forced: false, downloads: 5, matched: "hash" }],
      }),
      "POST /api/items/7/subtitles": () => track,
      "GET /api/files/3/subtitles/2000.vtt": () => new Response("WEBVTT\n\n00:00:01.000 --> 00:00:09.000\nFound it", { status: 200 }),
      "PUT /api/items/7/streams": () => undefined,
      "DELETE /api/items/7/subtitles/2000": () => undefined,
    });
    const { container } = renderPlayer(0);
    const video = container.querySelector("video")!;
    Object.defineProperty(video, "currentTime", { value: 2, configurable: true, writable: true });
    await waitFor(() => expect(hlsInstances).toHaveLength(1));

    await user.click(screen.getByRole("button", { name: "Playback settings" }));
    await user.click(screen.getByRole("button", { name: /Subtitles\s*None/ }));
    await user.click(screen.getByRole("menuitem", { name: "Find subtitles…" }));
    await user.click(await screen.findByRole("menuitem", { name: /Heat\.1995/ }));

    await waitFor(() => {
      fireEvent.timeUpdate(video);
      expect(screen.getByTestId("subtitles")).toHaveTextContent("Found it");
    });
    // Chosen for the show, as any track picked; the same session plays on.
    expect(requests.find((r) => r.method === "PUT" && r.path === "/api/items/7/streams")?.body).toMatchObject({ subtitle_index: 2000 });
    expect(requests.filter((r) => r.method === "POST" && r.path === "/api/playback")).toHaveLength(1);

    await user.click(screen.getByRole("button", { name: "Playback settings" }));
    await user.click(screen.getByRole("button", { name: /Subtitles\s*English/ }));
    await user.click(screen.getByRole("button", { name: /^Delete English/ }));
    await user.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/items/7/subtitles/2000")).toBe(true));
    fireEvent.timeUpdate(video);
    await waitFor(() => expect(screen.queryByTestId("subtitles")).not.toBeInTheDocument());
    // Gone from the menu, which stays open, and turned off.
    expect(screen.queryByRole("menuitemradio", { name: /English/ })).not.toBeInTheDocument();
    expect(screen.getByRole("menuitemradio", { name: "Off" })).toHaveAttribute("aria-checked", "true");
  });
});

describe("VideoPlayer obeying the remote", () => {
  beforeEach(() => {
    vi.stubGlobal("EventSource", FakeEventSource);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  /** The player's controls are on screen: its top bar, which shows and hides with the rest. */
  const controlsShown = () => !screen.getByRole("button", { name: "Back" }).parentElement!.classList.contains("opacity-0");
  /** The controls' timer, faked, so a test can let them hide as they do after a while. */
  const fakeTimers = () => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"], shouldAdvanceTime: true });
  /** It plays, and nobody touches it here: the controls go. */
  async function untouched(video: HTMLVideoElement) {
    fireEvent.play(video);
    act(() => {
      vi.advanceTimersByTime(3_000);
    });
    await waitFor(() => expect(controlsShown()).toBe(false));
  }

  const command = (
    command: Extract<ServerEvent, { type: "player_command" }>["command"],
    more: { session_id?: string; position_ms?: number; offset_ms?: number; stream_index?: number; quality?: string } = {},
  ): ServerEvent => ({
    type: "player_command",
    session_id: more.session_id ?? "s1",
    command,
    position_ms: more.position_ms ?? null,
    offset_ms: more.offset_ms ?? null,
    stream_index: more.stream_index ?? null,
    quality: more.quality ?? null,
  });
  const reports = (requests: FakeRequest[]) =>
    requests.filter((r) => r.method === "POST" && r.path === "/api/items/7/progress").map((r) => r.body);

  /** The player, started at `startMs`, playing (as far as it can tell) its first session, `s1`. */
  async function playing(startMs = 0, item = detail(), first: Partial<PlaybackInfo> = {}) {
    const requests = serve([session("s1", first), session("s2")], {
      [`POST /api/items/${item.id}/progress`]: () => undefined,
      [`PUT /api/items/${item.id}/streams`]: () => undefined,
      "DELETE /api/playback/s1": () => undefined,
    });
    function Player() {
      useServerEvents(testUser.id);
      return (
        <>
          <VideoPlayer item={item} startMs={startMs} fileId={null} audio={null} sub={null} />
          <LocationDisplay />
        </>
      );
    }
    const { container, unmount } = renderWithProviders(<Player />);
    await waitFor(() => expect(hlsInstances[0]?.calls).toContain("attachMedia"));
    const video = container.querySelector("video")!;
    let paused = false;
    Object.defineProperty(video, "paused", { configurable: true, get: () => paused });
    // Starting pauses the element too.
    media.pause.mockClear();
    return { requests, video, unmount, setPaused: (p: boolean) => (paused = p) };
  }

  /** A film with two audio tracks, the first its default, and a text subtitle track. */
  function tracked(): ItemDetail {
    const item = detail();
    item.files[0]!.streams = [
      makeStream({ index: 0, kind: "video" }),
      makeStream({ index: 1, kind: "audio", codec: "eac3", language: "eng", default: true }),
      makeStream({ index: 2, kind: "audio", codec: "aac", language: "fre", default: false }),
      makeStream({ index: 3, kind: "subtitle", codec: "subrip", language: "eng", default: false, text_based: true }),
    ];
    return item;
  }
  const starts = (requests: FakeRequest[]) => requests.filter((r) => r.method === "POST" && r.path === "/api/playback");
  const remembered = (requests: FakeRequest[]) =>
    requests.filter((r) => r.method === "PUT" && r.path === "/api/items/7/streams").map((r) => r.body);

  it("says what it obeys as it starts: Next only where something comes next, Audio where there is a choice", async () => {
    const { requests } = await playing();
    expect(requests.find((r) => r.path === "/api/playback")?.body).toMatchObject({
      controls: ["play", "pause", "seek", "skip", "subtitles", "quality"],
    });
  });

  it("takes a change of audio where the file has a choice of tracks", async () => {
    const { requests } = await playing(0, tracked());
    expect(requests.find((r) => r.path === "/api/playback")?.body).toMatchObject({
      controls: ["play", "pause", "seek", "skip", "audio", "subtitles", "quality"],
    });
  });

  it("changes quality as its own menu does, keeps it, and says in its reports which it plays", async () => {
    const { requests, video } = await playing(0, tracked(), { audio_index: 1 });
    fireEvent.pause(video);
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ quality: "auto" }));
    // The one it plays already: nothing to say.
    act(() => FakeEventSource.last?.send(command("quality", { quality: "auto" })));
    act(() => FakeEventSource.last?.send(command("quality", { quality: "2m" })));
    await waitFor(() => expect(starts(requests)).toHaveLength(2));
    expect(starts(requests)[1]?.body).toMatchObject({ max_bitrate: 2_000 });
    expect(localStorage.getItem("pley.quality")).toContain("2m");
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ quality: "2m" }));
    // Said as its Quality menu names it.
    expect(screen.getAllByText(/^Quality:/).map((n) => n.textContent)).toEqual(["Quality: 2 Mbps 480p"]);
    // One its menu doesn't offer is left alone.
    act(() => FakeEventSource.last?.send(command("quality", { quality: "nonsense" })));
    await act(() => new Promise((r) => setTimeout(r, 50)));
    expect(starts(requests)).toHaveLength(2);
    expect(screen.getAllByText(/^Quality:/)).toHaveLength(1);
  });

  it("plays its session in this tab until it is stopped, so the remote leaves it out", async () => {
    await playing();
    await waitFor(() => expect(isPlayingHere("s1")).toBe(true));
    act(() => FakeEventSource.last?.send({ type: "session_stopped", session_id: "s1", message: null, by_admin: false }));
    await waitFor(() => expect(isPlayingHere("s1")).toBe(false));
  });

  it("leaves a command for another session alone", async () => {
    const { requests, video } = await playing(60_000);
    const before = requests.length;
    act(() => FakeEventSource.last?.send(command("pause", { session_id: "other" })));
    act(() => FakeEventSource.last?.send(command("seek", { session_id: "other", position_ms: 1_000 })));
    expect(media.pause).not.toHaveBeenCalled();
    expect(video.currentTime).not.toBe(1);
    expect(requests.slice(before)).toEqual([]);
    expect(screen.queryByText("Paused")).not.toBeInTheDocument();
    expect(screen.queryByText(/^Jumped to/)).not.toBeInTheDocument();
  });

  it("pauses and plays, says it plays again at once, and says each in a brief notice", async () => {
    const { requests, setPaused } = await playing();
    act(() => FakeEventSource.last?.send(command("pause")));
    expect(media.pause).toHaveBeenCalled();
    expect(await screen.findByText("Paused")).toBeInTheDocument();

    setPaused(true);
    media.play.mockClear();
    act(() => FakeEventSource.last?.send(command("play")));
    expect(media.play).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ state: "playing", session_id: "s1" }));
    expect(await screen.findByText("Playing")).toBeInTheDocument();
  });

  it("says again that it plays, or is paused, when told so already, so a remote a little behind settles", async () => {
    const { requests, setPaused } = await playing();
    media.play.mockClear();
    act(() => FakeEventSource.last?.send(command("play")));
    expect(media.play).not.toHaveBeenCalled();
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ state: "playing", session_id: "s1" }));

    setPaused(true);
    act(() => FakeEventSource.last?.send(command("pause")));
    expect(media.pause).not.toHaveBeenCalled();
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ state: "paused", session_id: "s1" }));
    // Nothing changed here: nothing to say.
    expect(screen.queryByText("Playing")).not.toBeInTheDocument();
    expect(screen.queryByText("Paused")).not.toBeInTheDocument();
  });

  it("seeks, no further than the end, reports where at once, and says where to", async () => {
    const { requests, video } = await playing();
    act(() => FakeEventSource.last?.send(command("seek", { position_ms: 754_000 })));
    expect(video.currentTime).toBe(754);
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ position_ms: 754_000, state: "playing" }));
    expect(await screen.findByText("Jumped to 12:34")).toBeInTheDocument();

    act(() => FakeEventSource.last?.send(command("seek", { position_ms: 3_723_000 })));
    expect(await screen.findByText("Jumped to 1:02:03")).toBeInTheDocument();

    act(() => FakeEventSource.last?.send(command("seek", { position_ms: 99_000_000 })));
    expect(video.currentTime).toBeLessThanOrEqual(6_000);
    expect(video.currentTime).toBeGreaterThan(5_999);
    // Where it went: the end.
    expect(await screen.findByText("Jumped to 1:39:59")).toBeInTheDocument();
  });

  it("skips from where it is, never before the start, and says by how much", async () => {
    const { requests, video, setPaused } = await playing(60_000);
    setPaused(true);
    act(() => FakeEventSource.last?.send(command("skip", { offset_ms: 30_000 })));
    expect(video.currentTime).toBe(90);
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ position_ms: 90_000, state: "paused" }));
    expect(await screen.findByText("+30 s")).toBeInTheDocument();

    act(() => FakeEventSource.last?.send(command("skip", { offset_ms: -10_000 })));
    expect(video.currentTime).toBe(80);
    expect(await screen.findByText("−10 s")).toBeInTheDocument();

    act(() => FakeEventSource.last?.send(command("skip", { offset_ms: -600_000 })));
    expect(video.currentTime).toBe(0);
  });

  it("shows a remote's pause, skip or seek only in its notice: the controls stay hidden", async () => {
    fakeTimers();
    const { video, setPaused } = await playing(60_000);
    await untouched(video);

    act(() => FakeEventSource.last?.send(command("pause")));
    setPaused(true);
    fireEvent.pause(video);
    expect(await screen.findByText("Paused")).toBeInTheDocument();
    act(() => FakeEventSource.last?.send(command("skip", { offset_ms: 30_000 })));
    act(() => FakeEventSource.last?.send(command("seek", { position_ms: 754_000 })));
    expect(await screen.findByText("Jumped to 12:34")).toBeInTheDocument();
    expect(controlsShown()).toBe(false);

    // Someone here presses a key: the controls show, and stay while it's paused.
    fireEvent.keyDown(video, { key: "Shift" });
    expect(controlsShown()).toBe(true);
    act(() => {
      vi.advanceTimersByTime(5_000);
    });
    expect(controlsShown()).toBe(true);
  });

  it("shows the controls for a pause here as before, and a remote's pause ends as it plays again", async () => {
    fakeTimers();
    const { video, setPaused } = await playing();
    await untouched(video);

    // Paused here (a media key, say): the controls show, as they always have.
    fireEvent.pause(video);
    expect(controlsShown()).toBe(true);
    fireEvent.play(video);
    expect(controlsShown()).toBe(false);

    act(() => FakeEventSource.last?.send(command("pause")));
    setPaused(true);
    fireEvent.pause(video);
    expect(controlsShown()).toBe(false);
    act(() => FakeEventSource.last?.send(command("play")));
    setPaused(false);
    fireEvent.play(video);
    expect(await screen.findByText("Playing")).toBeInTheDocument();
    expect(controlsShown()).toBe(false);

    // Played again: the next pause here shows them, as before.
    fireEvent.pause(video);
    expect(controlsShown()).toBe(true);
  });

  it("asks for a click, with the controls, where the browser won't play a remote's Play without one", async () => {
    fakeTimers();
    const { video, setPaused } = await playing();
    await untouched(video);
    act(() => FakeEventSource.last?.send(command("pause")));
    setPaused(true);
    fireEvent.pause(video);
    expect(controlsShown()).toBe(false);

    media.play.mockRejectedValueOnce(new DOMException("Not allowed", "NotAllowedError"));
    act(() => FakeEventSource.last?.send(command("play")));
    // Its big Play, beside the controls' own.
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Play" })).toHaveLength(2));
    expect(controlsShown()).toBe(true);
    expect(screen.queryByText("Playing")).not.toBeInTheDocument();
  });

  it("plays the next episode, as its own Next does", async () => {
    const episode = detail({ id: 8, kind: "episode", title: "Pilot" });
    const next = makeItem({ id: 9, kind: "episode", title: "Second" });
    const { requests } = await playing(0, episode);
    expect(requests.find((r) => r.path === "/api/playback")?.body).toMatchObject({
      controls: ["play", "pause", "seek", "skip", "next", "subtitles", "quality"],
    });
    mockApi({
      "GET /api/items/8/next": () => ({ item: next }),
      "POST /api/items/8/progress": () => undefined,
      "DELETE /api/playback/s1": () => undefined,
    });
    act(() => FakeEventSource.last?.send(command("next")));
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(/^\/play\//));
  });

  it("does nothing for Next when nothing comes next", async () => {
    const episode = detail({ id: 8, kind: "episode", title: "Finale" });
    await playing(0, episode);
    const requests = mockApi({ "GET /api/items/8/next": () => ({ item: null }), "POST /api/items/8/progress": () => undefined });
    act(() => FakeEventSource.last?.send(command("next")));
    await waitFor(() => expect(requests.some((r) => r.path === "/api/items/8/next")).toBe(true));
    expect(screen.getByTestId("location")).toHaveTextContent(/^\/$/);
    expect(screen.queryByText("This is the last episode")).not.toBeInTheDocument();
  });

  it("changes audio as its own menu does: a new session with the track picked, which it remembers", async () => {
    const { requests } = await playing(0, tracked(), { audio_index: 1 });
    act(() => FakeEventSource.last?.send(command("audio", { stream_index: 2 })));
    await waitFor(() => expect(starts(requests)).toHaveLength(2));
    expect(starts(requests)[1]?.body).toMatchObject({ audio_index: 2 });
    await waitFor(() => expect(remembered(requests)).toEqual([{ file_id: 3, audio_index: 2, subtitle_index: null }]));
    // Said as its Audio menu names the track.
    expect(screen.getByText("Audio: French (AAC)")).toBeInTheDocument();
  });

  it("says at once that it plays the audio picked, where it does already", async () => {
    const { requests } = await playing(0, tracked(), { audio_index: 1 });
    const before = reports(requests).length;
    act(() => FakeEventSource.last?.send(command("audio", { stream_index: 1 })));
    await waitFor(() => expect(reports(requests)).toHaveLength(before + 1));
    expect(reports(requests).at(-1)).toMatchObject({ state: "playing", session_id: "s1", audio_index: 1, subtitle_index: -1 });
    expect(starts(requests)).toHaveLength(1);
    expect(screen.queryByText(/^Audio:/)).not.toBeInTheDocument();
  });

  it("turns subtitles on and off as its own menu does, and says so at once", async () => {
    const { requests, setPaused } = await playing(0, tracked(), { audio_index: 1 });
    setPaused(true);
    act(() => FakeEventSource.last?.send(command("subtitles", { stream_index: 3 })));
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ state: "paused", audio_index: 1, subtitle_index: 3 }));
    await waitFor(() => expect(remembered(requests)).toEqual([{ file_id: 3, subtitle_index: 3 }]));
    expect(screen.getByText("Subtitles: English (SRT)")).toBeInTheDocument();

    act(() => FakeEventSource.last?.send(command("subtitles", { stream_index: -1 })));
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ subtitle_index: -1 }));
    expect(screen.getByText("Subtitles off")).toBeInTheDocument();
    // Off already: said again at once, so a remote a little behind settles, with nothing to say here.
    const before = reports(requests).length;
    act(() => FakeEventSource.last?.send(command("subtitles", { stream_index: -1 })));
    await waitFor(() => expect(reports(requests)).toHaveLength(before + 1));
    expect(reports(requests).at(-1)).toMatchObject({ subtitle_index: -1 });
    expect(screen.getAllByText(/^Subtitles/).map((n) => n.textContent)).toEqual(["Subtitles: English (SRT)", "Subtitles off"]);
    // Text subtitles need no new session.
    expect(starts(requests)).toHaveLength(1);
  });

  it("says in every report the audio and the subtitles it plays, its last one too", async () => {
    const { requests, video, unmount } = await playing(0, tracked(), { audio_index: 2 });
    fireEvent.pause(video);
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ state: "paused", audio_index: 2, subtitle_index: -1 }));
    unmount();
    await waitFor(() => expect(reports(requests).at(-1)).toMatchObject({ state: "stopped", audio_index: 2, subtitle_index: -1 }));
  });
});
