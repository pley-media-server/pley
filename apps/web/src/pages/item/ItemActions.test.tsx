import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ItemDetail, MediaFile } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { renderWithProviders, testUser } from "@/test/render";
import { MoreActions } from "./ItemActions";

const viewer = { ...testUser, is_admin: false };

function file(id: number, overrides: Partial<MediaFile> = {}): MediaFile {
  return {
    id,
    path: `/media/Movies/Vertigo (1958)/Vertigo.1958.${id}.mkv`,
    size: 8_804_682_752,
    container: "matroska,webm",
    duration_ms: null,
    bitrate: null,
    width: null,
    height: null,
    video_codec: null,
    audio_codec: null,
    streams: [],
    edition: null,
    selected_audio: null,
    selected_subtitle: null,
    chapters: [],
    ...overrides,
  };
}

function movie(files: MediaFile[]): ItemDetail {
  return {
    ...makeItem({ id: 5, kind: "movie", title: "Vertigo", year: 1958 }),
    original_title: null,
    tagline: null,
    genres: [],
    studio: null,
    content_rating: null,
    tmdb_id: null,
    imdb_id: null,
    files,
    in_watchlist: false,
  };
}

describe("MoreActions", () => {
  afterEach(() => vi.restoreAllMocks());

  it("downloads the version the page plays, named for what it is", async () => {
    const user = userEvent.setup();
    const clicked: HTMLAnchorElement[] = [];
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) {
      clicked.push(this);
    });
    const item = movie([file(7), file(8, { size: 2_147_483_648, edition: "Director's Cut" })]);
    renderWithProviders(<MoreActions item={item} file={item.files[1]} />, { user: viewer });

    await user.click(screen.getByRole("button", { name: "More actions" }));
    await user.click(screen.getByRole("menuitem", { name: /Download\s*2\.0 GB/ }));

    expect(clicked.map((a) => [a.getAttribute("href"), a.download])).toEqual([
      [`/api/files/8/stream?item=${item.id}`, "Vertigo (1958) - Director's Cut.mkv"],
    ]);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("has nothing to download without a file", () => {
    renderWithProviders(<MoreActions item={movie([])} />, { user: viewer });
    expect(screen.queryByRole("button", { name: "More actions" })).toBeNull();
  });
});
