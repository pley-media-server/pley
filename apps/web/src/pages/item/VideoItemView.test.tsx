import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ItemDetail, MediaFile } from "@/api/types";
import { makeItem, makeStream } from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import { VideoItemView } from "./VideoItemView";

const saveStreamChoice = vi.hoisted(() => vi.fn<(itemId: number, body: unknown) => Promise<void>>(() => Promise.resolve()));
vi.mock("@/lib/progress", async (importOriginal) => ({ ...(await importOriginal<object>()), saveStreamChoice }));

function file(overrides: Partial<MediaFile> = {}): MediaFile {
  return {
    id: 7,
    path: "Vertigo.1958.mkv",
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
    ...makeItem({ id: 5, kind: "movie", title: "Vertigo", year: 1958 }),
    original_title: null,
    tagline: null,
    genres: [],
    studio: null,
    content_rating: null,
    tmdb_id: 426,
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
    const dv = file({ id: 8, path: "Vertigo.1958.DV.mkv", streams: [video({ dynamic_range: "dolby_vision", dv_profile: 8, dv_bl_compat: 1 })] });
    renderWithProviders(<VideoItemView item={{ ...movie(hdr10), files: [hdr10, dv] }} />);
    expect(screen.getByText("HDR10")).toBeInTheDocument();
    await user.selectOptions(screen.getByRole("combobox", { name: "Version" }), "8");
    expect(screen.getByText("DV P8.1")).toBeInTheDocument();
    expect(screen.queryByText("HDR10")).not.toBeInTheDocument();
  });
});
