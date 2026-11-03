import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useServerEvents } from "@/api/events";
import type { Channel } from "@/api/livetv";
import type { ServerEvent } from "@/api/types";
import { mockApi, renderWithProviders, testUser, type FakeRequest } from "@/test/render";
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

vi.mock("@/lib/hls", () => ({ loadHls: () => Promise.resolve(FakeHls) }));
// No MPEG-TS player: every channel plays from the transcoder's HLS.
vi.mock("@/lib/mpegts", () => ({ loadMpegts: () => Promise.reject(new Error("not here")) }));

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
