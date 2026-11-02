import { afterEach, describe, expect, it, vi } from "vitest";
import type { ItemDetail, MediaFile } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { downloadName, extensionOf, fileUrl, saveFile } from "./download";

function file(overrides: Partial<MediaFile> = {}): MediaFile {
  return {
    id: 7,
    path: "/media/Movies/Vertigo (1958)/Vertigo.1958.1080p.BluRay.x264-GRP.mkv",
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

function detail(overrides: Parameters<typeof makeItem>[0]): ItemDetail {
  return {
    ...makeItem(overrides),
    original_title: null,
    tagline: null,
    genres: [],
    studio: null,
    content_rating: null,
    tmdb_id: null,
    imdb_id: null,
    files: [],
    in_watchlist: false,
  };
}

describe("downloadName", () => {
  it("names a download by what it is, not by the file on the server", () => {
    expect(downloadName(detail({ kind: "movie", title: "Vertigo", year: 1958 }), file())).toBe("Vertigo (1958).mkv");
    expect(
      downloadName(detail({ kind: "movie", title: "Rear Window", year: 1954 }), file({ edition: "Extended Cut", path: "x.mp4" })),
    ).toBe("Rear Window (1954) - Extended Cut.mp4");
    const episode = detail({ kind: "episode", title: "Dulcinea", grandparent_title: "The Expanse", parent_index: 1, index: 1 });
    expect(downloadName(episode, file())).toBe("The Expanse - S01E01 - Dulcinea.mkv");
    const track = detail({ kind: "track", title: "Airbag", grandparent_title: "Radiohead" });
    expect(downloadName(track, file({ path: "D:\\Music\\Radiohead\\01 Airbag.FLAC" }))).toBe("Radiohead - Airbag.flac");
  });

  it("leaves out what file names can't have", () => {
    expect(downloadName(detail({ kind: "movie", title: 'What? "Now": A/B Story...' }), file())).toBe("What Now A B Story.mkv");
    expect(downloadName(detail({ kind: "movie", title: "???" }), file())).toBe("Download.mkv");
  });
});

describe("extensionOf", () => {
  it("takes the file's own extension, else its container", () => {
    expect(extensionOf({ path: "C:\\Films\\Jaws.M2TS", container: null })).toBe("m2ts");
    expect(extensionOf({ path: "/films/Jaws", container: "mp4" })).toBe("mp4");
    expect(extensionOf({ path: "/films/Jaws", container: null })).toBe("mkv");
  });
});

describe("saveFile", () => {
  afterEach(() => vi.restoreAllMocks());

  it("downloads the original under its name", () => {
    const clicked: HTMLAnchorElement[] = [];
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) {
      clicked.push(this);
    });
    saveFile(fileUrl({ id: 7 }), "Vertigo (1958).mkv");
    expect(clicked).toHaveLength(1);
    expect(clicked[0]?.getAttribute("href")).toBe("/api/files/7/stream");
    expect(clicked[0]?.download).toBe("Vertigo (1958).mkv");
    expect(clicked[0]?.isConnected).toBe(false);
  });

  it("names the title for the desktop app, which asks for a file its player plays", () => {
    expect(fileUrl({ id: 7 }, { id: 42 })).toBe("/api/files/7/stream?item=42");
  });
});
