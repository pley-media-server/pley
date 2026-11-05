import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ItemDetail, MediaFile } from "@/api/types";
import { makeItem, makeReceiver, makeStream } from "@/test/fixtures";
import { mockApi, renderWithProviders } from "@/test/render";
import { VideoItemView } from "./VideoItemView";

const saveStreamChoice = vi.hoisted(() => vi.fn<(itemId: number, body: unknown) => Promise<void>>(() => Promise.resolve()));
vi.mock("@/lib/progress", async (importOriginal) => ({ ...(await importOriginal<object>()), saveStreamChoice }));

function file(overrides: Partial<MediaFile> = {}): MediaFile {
  return {
    id: 7,
    path: "Alien.1979.mkv",
    size: 8_804_682_752,
    container: "mkv",
    duration_ms: null,
    bitrate: null,
    width: null,
    height: null,
    video_codec: null,
    audio_codec: null,
    streams: [
      makeStream({ index: 0, kind: "video", codec: "hevc" }),
      makeStream({ index: 1, kind: "audio", codec: "truehd", language: "eng", channels: 6, default: true }),
      makeStream({ index: 2, kind: "audio", codec: "aac", language: "eng", channels: 2, default: false }),
    ],
    edition: null,
    selected_audio: 1,
    selected_subtitle: null,
    audio_picked: false,
    chapters: [],
    ...overrides,
  };
}

function movie(f: MediaFile): ItemDetail {
  return {
    ...makeItem({ id: 5, kind: "movie", title: "Alien", year: 1979 }),
    original_title: null,
    tagline: null,
    genres: [],
    studio: null,
    content_rating: null,
    tmdb_id: 348,
    imdb_id: null,
    files: [f],
    in_watchlist: false,
    cast: [],
    crew: [],
    extras: [],
  };
}

const audioMenu = () => screen.getByRole<HTMLSelectElement>("combobox", { name: "Audio" });

describe("the title page's audio", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(new Response("[]", { headers: { "Content-Type": "application/json" } }))));
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    saveStreamChoice.mockClear();
  });

  it("reads Automatic with its language while nothing is picked, not the details' track", () => {
    renderWithProviders(<VideoItemView item={movie(file())} />);
    const menu = audioMenu();
    expect(menu.selectedOptions[0]?.textContent).toBe("Automatic (English)");
    expect(within(menu).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Automatic (English)",
      "English (TrueHD 5.1)",
      "English (AAC Stereo)",
    ]);
  });

  it("ticks a picked track", () => {
    renderWithProviders(<VideoItemView item={movie(file({ selected_audio: 2, audio_picked: true }))} />);
    expect(audioMenu().selectedOptions[0]?.textContent).toBe("English (AAC Stereo)");
  });

  it("picks a track, and Automatic picks none", async () => {
    const user = userEvent.setup();
    renderWithProviders(<VideoItemView item={movie(file())} />);

    await user.selectOptions(audioMenu(), "English (AAC Stereo)");
    expect(audioMenu().selectedOptions[0]?.textContent).toBe("English (AAC Stereo)");
    expect(saveStreamChoice).toHaveBeenLastCalledWith(5, { file_id: 7, audio_index: 2, subtitle_index: null });

    await user.selectOptions(audioMenu(), "Automatic (English)");
    expect(audioMenu().selectedOptions[0]?.textContent).toBe("Automatic (English)");
    expect(saveStreamChoice).toHaveBeenLastCalledWith(5, { file_id: 7, audio_index: null, subtitle_index: null });
  });
});

describe("the title page's Play on…", () => {
  afterEach(() => vi.unstubAllGlobals());

  /** What casting a film in `user_state` to the living room TV asks it to play, after `before` on the page. */
  async function castFrom(
    user_state: ItemDetail["user_state"],
    f: MediaFile = file(),
    before: (user: ReturnType<typeof userEvent.setup>) => Promise<void> = () => Promise.resolve(),
  ) {
    const requests = mockApi({
      "GET /api/sessions": () => [],
      "GET /api/remote/devices": () => [makeReceiver()],
      "POST /api/remote/devices/tv-1/cast": () => undefined,
      "PUT /api/items/5/streams": () => undefined,
    });
    const user = userEvent.setup();
    const { unmount } = renderWithProviders(<VideoItemView item={{ ...movie(f), title: "Test Pattern", user_state }} />);
    await before(user);
    await user.click(await screen.findByRole("button", { name: "Play on…" }));
    await user.click(screen.getByRole("menuitem", { name: "Living room TV" }));
    await waitFor(() => expect(requests.some((r) => r.method === "POST")).toBe(true));
    unmount();
    return requests.find((r) => r.method === "POST")?.body;
  }

  it("plays it on another device from where Play would resume, and as Play would otherwise", async () => {
    expect(await castFrom({ position_ms: 90_000, watched: false, play_count: 0 })).toEqual({ item_id: 5, start_ms: 90_000 });
    expect(await castFrom({ position_ms: 0, watched: true, play_count: 1 })).toEqual({ item_id: 5 });
  });

  it("sends the audio picked here, and the subtitles shown here (Off included)", async () => {
    const watched = { position_ms: 0, watched: true, play_count: 1 };
    const subtitled = file({
      streams: [
        ...file().streams,
        makeStream({ index: 3, kind: "subtitle", codec: "subrip", language: "fre", default: false }),
      ],
    });
    expect(await castFrom(watched, subtitled)).toEqual({ item_id: 5, subtitle_index: -1 });
    const picked = await castFrom(watched, subtitled, async (user) => {
      await user.selectOptions(audioMenu(), "English (AAC Stereo)");
    });
    expect(picked).toEqual({ item_id: 5, audio_index: 2, subtitle_index: -1 });
  });
});

describe("the title page's quality badges", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(new Response("[]", { headers: { "Content-Type": "application/json" } }))));
  });
  afterEach(() => vi.unstubAllGlobals());

  it("follow the version picked", async () => {
    const user = userEvent.setup();
    const video = (range: Partial<MediaFile["streams"][number]>) =>
      makeStream({ index: 0, kind: "video", codec: "hevc", width: 3840, height: 2160, ...range });
    const hdr10 = file({ id: 7, streams: [video({ dynamic_range: "hdr10" })] });
    const dv = file({ id: 8, path: "Alien.1979.DV.mkv", streams: [video({ dynamic_range: "dolby_vision", dv_profile: 8, dv_bl_compat: 1 })] });
    renderWithProviders(<VideoItemView item={{ ...movie(hdr10), files: [hdr10, dv] }} />);
    expect(screen.getByText("HDR10")).toBeInTheDocument();
    await user.selectOptions(screen.getByRole("combobox", { name: "Version" }), "8");
    expect(screen.getByText("DV P8.1")).toBeInTheDocument();
    expect(screen.queryByText("HDR10")).not.toBeInTheDocument();
  });
});
