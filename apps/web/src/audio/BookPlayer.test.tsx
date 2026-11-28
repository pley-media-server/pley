import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ItemSummary, PlaybackInfo } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { mockApi, renderWithProviders, type FakeRequest } from "@/test/render";
import { AudioPlayerProvider, useAudioPlayer } from "./AudioPlayer";
import { AudioPlayerBar } from "./AudioPlayerBar";

const parts: ItemSummary[] = [
  makeItem({ id: 31, kind: "part", parent_id: 30, title: "Chapter One", duration_ms: 1_800_000 }),
  makeItem({ id: 32, kind: "part", parent_id: 30, title: "Chapter Two", duration_ms: 1_800_000 }),
];
const song = makeItem({ id: 21, kind: "track", title: "Song", duration_ms: 200_000 });

function session(id: string): PlaybackInfo {
  return {
    session_id: id,
    method: "direct",
    url: `/api/playback/${id}/file`,
    hls: false,
    file_id: 1,
    duration_ms: 1_800_000,
    audio_index: null,
    subtitle_index: null,
    burn_subtitles: false,
    subtitles: [],
    reason: null,
  };
}

function serve(): FakeRequest[] {
  let n = 0;
  return mockApi({
    "POST /api/playback": () => session(`s${++n}`),
    "POST /api/items/31/progress": () => undefined,
    "POST /api/items/32/progress": () => undefined,
    "POST /api/items/21/progress": () => undefined,
    "DELETE /api/playback/s1": () => undefined,
    "DELETE /api/playback/s2": () => undefined,
    "GET /api/items/32": () => ({
      ...parts[1],
      files: [
        {
          id: 1,
          chapters: [
            { start_ms: 0, end_ms: 600_000, title: "Opening Credits" },
            { start_ms: 600_000, end_ms: 1_800_000, title: "Chapter 1" },
          ],
        },
      ],
    }),
  });
}

/** Buttons that drive the player as the book page and the bar do. */
function Controls() {
  const player = useAudioPlayer();
  return (
    <>
      <button type="button" onClick={() => player.playQueue(parts, 1, { startAtMs: 600_000 })}>
        Resume
      </button>
      <button type="button" onClick={() => player.playQueue([song])}>
        Song
      </button>
      <button type="button" onClick={() => player.setRate(1.5)}>
        Faster
      </button>
      <button type="button" onClick={() => player.skipBy(-15)}>
        Back
      </button>
      <span data-testid="book">{String(player.book)}</span>
    </>
  );
}

const starts = (requests: FakeRequest[]) => requests.filter((r) => r.method === "POST" && r.path === "/api/playback");

describe("AudioPlayer with an audiobook", () => {
  beforeEach(() => {
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => undefined);
    localStorage.clear();
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  async function resumed() {
    const requests = serve();
    const { container } = renderWithProviders(
      <AudioPlayerProvider>
        <Controls />
      </AudioPlayerProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Resume" }));
    const audio = container.querySelector("audio")!;
    await waitFor(() => expect(audio.getAttribute("src")).toBe("/api/playback/s1/file"));
    return { requests, audio };
  }

  it("takes a book up in the part it was left in, where it was left", async () => {
    const { requests } = await resumed();
    expect(starts(requests)[0]?.body).toMatchObject({ item_id: 32, start_ms: 600_000 });
    expect(screen.getByTestId("book")).toHaveTextContent("true");
  });

  it("plays a book at its speed, kept for next time, and music at its own", async () => {
    const { audio } = await resumed();
    expect(audio.playbackRate).toBe(1);
    await userEvent.click(screen.getByRole("button", { name: "Faster" }));
    expect(audio.playbackRate).toBe(1.5);
    expect(audio.defaultPlaybackRate).toBe(1.5);
    expect(localStorage.getItem("pley.audiobook.rate")).toBe("1.5");

    await userEvent.click(screen.getByRole("button", { name: "Song" }));
    await waitFor(() => expect(audio.getAttribute("src")).toBe("/api/playback/s2/file"));
    expect(screen.getByTestId("book")).toHaveTextContent("false");
    expect(audio.playbackRate).toBe(1);
  });

  it("skips back, never before the start", async () => {
    const { audio } = await resumed();
    audio.currentTime = 10;
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(audio.currentTime).toBe(0);
  });

  it("runs the bar through the chapter playing, named under the book", async () => {
    const requests = serve();
    const { container } = renderWithProviders(
      <AudioPlayerProvider>
        <Controls />
        <AudioPlayerBar />
      </AudioPlayerProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Resume" }));
    const audio = container.querySelector("audio")!;
    await waitFor(() => expect(audio.getAttribute("src")).toBe("/api/playback/s1/file"));
    expect(requests.some((r) => r.path === "/api/items/32")).toBe(true);
    Object.defineProperty(audio, "readyState", { configurable: true, get: () => 4 });
    Object.defineProperty(audio, "duration", { configurable: true, get: () => 1800 });
    audio.currentTime = 900;
    act(() => {
      audio.dispatchEvent(new Event("timeupdate"));
    });
    // Five minutes into a chapter of twenty.
    expect(await screen.findByText("Chapter 1")).toBeInTheDocument();
    expect(screen.getByText("5:00")).toBeInTheDocument();
    expect(screen.getByText("20:00")).toBeInTheDocument();
  });
});
