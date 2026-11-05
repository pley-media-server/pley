import { useState } from "react";
import { Route, Routes, useNavigate } from "react-router";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "@/api/hooks";
import type { ItemDetail } from "@/api/types";
import { castState } from "@/lib/castReceiver";
import { encodeId } from "@/lib/ids";
import { itemUrl } from "@/lib/urls";
import { makeItem } from "@/test/fixtures";
import { LocationDisplay, mockApi, renderWithProviders, testQueryClient } from "@/test/render";
import { PlayerPage } from "./PlayerPage";

// The web player needs a real video element; here it only shows that it's
// there, and the subtitles it starts with (it reads them once, as it mounts).
const mounted = vi.hoisted(() => [] as (number | null)[]);
vi.mock("./VideoPlayer", () => ({
  VideoPlayer: ({ item }: { item: ItemDetail }) => {
    const [sub] = useState(() => {
      const first = item.files[0]?.selected_subtitle ?? null;
      mounted.push(first);
      return first;
    });
    return (
      <div data-testid="web-player">
        {item.title} · {sub ?? "off"}
      </div>
    );
  },
}));

const ANDROID = "Mozilla/5.0 (Linux; Android 15; wv) Chrome/140.0 Mobile Safari/537.36 pleyAndroid/0.1.0";

function detail(overrides: Partial<ItemDetail> = {}): ItemDetail {
  return {
    ...makeItem({ id: 7, kind: "episode", title: "Pilot", grandparent_title: "Taxi" }),
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
        path: "/tv/Taxi/S01E01.mkv",
        size: 1,
        container: "mkv",
        duration_ms: 1_500_000,
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

function renderPlayer(route: string) {
  mockApi({ "GET /api/items/7": () => detail() });
  renderWithProviders(
    <Routes>
      <Route path="/play/:id" element={<PlayerPage />} />
      <Route path="*" element={<LocationDisplay />} />
    </Routes>,
    { route },
  );
}

describe("PlayerPage in the Android app", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    delete (window as unknown as { PleyAndroid?: unknown }).PleyAndroid;
  });

  it("hands the video to the app's player and steps back", async () => {
    vi.stubGlobal("navigator", { ...navigator, userAgent: ANDROID });
    const play = vi.fn<(request: string) => boolean>(() => true);
    (window as unknown as { PleyAndroid: unknown }).PleyAndroid = { play };
    renderPlayer(`/play/${encodeId(7)}?t=90000&audio=1`);

    await waitFor(() => expect(play).toHaveBeenCalledTimes(1));
    expect(JSON.parse(play.mock.calls[0]?.[0] ?? "null")).toEqual({
      item_id: 7,
      start_ms: 90_000,
      file_id: null,
      audio: 1,
      sub: null,
      title: "Taxi · Pilot",
    });
    expect(await screen.findByTestId("location")).toHaveTextContent(itemUrl(7));
  });

  it("plays in the page when the app's player declines", async () => {
    vi.stubGlobal("navigator", { ...navigator, userAgent: ANDROID });
    const play = vi.fn<(request: string) => boolean>(() => false);
    (window as unknown as { PleyAndroid: unknown }).PleyAndroid = { play };
    renderPlayer(`/play/${encodeId(7)}`);
    expect(await screen.findByTestId("web-player")).toHaveTextContent("Pilot");
    expect(play).toHaveBeenCalledTimes(1);
  });
});

/** Opens the same title again: plainly, or as a cast. */
function Again() {
  const navigate = useNavigate();
  const again = `/play/${encodeId(7)}?t=60000`;
  return (
    <>
      <button type="button" onClick={() => void navigate(again)}>
        Same
      </button>
      <button type="button" onClick={() => void navigate(again, { state: castState() })}>
        Cast
      </button>
    </>
  );
}

describe("PlayerPage and casts", () => {
  it("starts the player again for a cast, though it plays that title already", async () => {
    mounted.length = 0;
    mockApi({ "GET /api/items/7": () => detail() });
    renderWithProviders(
      <Routes>
        <Route
          path="/play/:id"
          element={
            <>
              <PlayerPage />
              <Again />
            </>
          }
        />
      </Routes>,
      { route: `/play/${encodeId(7)}` },
    );
    await screen.findByTestId("web-player");
    fireEvent.click(screen.getByText("Same"));
    expect(mounted).toHaveLength(1);
    fireEvent.click(screen.getByText("Cast"));
    await waitFor(() => expect(mounted).toHaveLength(2));
  });
});

describe("PlayerPage and the tracks a title starts with", () => {
  const withSubtitle = (sub: number | null) => {
    const d = detail();
    return { ...d, files: d.files.map((f) => ({ ...f, selected_subtitle: sub })) };
  };

  it("waits for a copy known to be out of date, so the tracks picked since are the ones it starts with", async () => {
    mounted.length = 0;
    // Subtitles were turned on in the episode before: the server picks them now.
    mockApi({ "GET /api/items/7": () => withSubtitle(5) });
    const client = testQueryClient();
    client.setQueryData(keys.item(7), withSubtitle(null));
    await client.invalidateQueries({ queryKey: keys.item(7), refetchType: "none" });
    renderWithProviders(
      <Routes>
        <Route path="/play/:id" element={<PlayerPage />} />
      </Routes>,
      { route: `/play/${encodeId(7)}`, client },
    );
    expect(await screen.findByTestId("web-player")).toHaveTextContent("Pilot · 5");
    expect(mounted).toEqual([5]);
  });

  it("opens an older copy at once when nothing says it changed", async () => {
    mounted.length = 0;
    let answer: (value: ItemDetail) => void = () => undefined;
    mockApi({ "GET /api/items/7": () => new Promise<ItemDetail>((resolve) => (answer = resolve)) });
    const client = testQueryClient();
    client.setQueryData(keys.item(7), withSubtitle(2), { updatedAt: Date.now() - 3_600_000 });
    renderWithProviders(
      <Routes>
        <Route path="/play/:id" element={<PlayerPage />} />
      </Routes>,
      { route: `/play/${encodeId(7)}`, client },
    );
    expect(await screen.findByTestId("web-player")).toHaveTextContent("Pilot · 2");
    answer(withSubtitle(2));
  });
});
