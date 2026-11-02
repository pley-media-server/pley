import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MediaStream, SubtitleResult, SubtitleTrack } from "@/api/types";
import { apiError, mockApi, renderWithProviders, testUser } from "@/test/render";
import { forgetSubtitleSearch, type SubtitleSources } from "./FindSubtitles";
import { qualitiesFor } from "./quality";
import { SettingsPopover, type SettingsPopoverProps } from "./SettingsPopover";
import { DEFAULT_SUBTITLE_STYLE } from "./subtitles";

const result = (over: Partial<SubtitleResult>): SubtitleResult => ({
  id: "opensubtitles:1",
  provider: "opensubtitles",
  language: "en",
  name: "Jaws.1975.1080p.BluRay.x264-GRP",
  format: "srt",
  hearing_impaired: false,
  forced: false,
  downloads: 1234,
  matched: "hash",
  ...over,
});

const added: SubtitleTrack = {
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

function renderMenu(overrides: Partial<SettingsPopoverProps> = {}, sources: Partial<SubtitleSources> = {}, user = testUser) {
  const props: SettingsPopoverProps = {
    open: true,
    setOpen: vi.fn(),
    qualities: qualitiesFor(null),
    quality: "auto",
    originalHint: null,
    onQuality: vi.fn(),
    audioStreams: [],
    audioIndex: null,
    onAudio: vi.fn(),
    subtitleStreams: [],
    subtitleIndex: null,
    onSubtitle: vi.fn(),
    subtitleStyle: DEFAULT_SUBTITLE_STYLE,
    onSubtitleStyle: vi.fn(),
    subtitleOffset: 0,
    onSubtitleOffset: vi.fn(),
    subtitleSources: { itemId: 7, fileId: 3, language: "eng", isAdmin: user.is_admin, onAdded: vi.fn(), ...sources },
    canDeleteSubtitle: () => true,
    onDeleteSubtitle: vi.fn(),
    onSubtitleFileRefused: vi.fn(),
    speed: 1,
    onSpeed: vi.fn(),
    sleep: null,
    sleepLeft: null,
    sleepEndLabel: "End of this movie",
    onSleep: vi.fn(),
    ...overrides,
  };
  const view = renderWithProviders(<SettingsPopover {...props} />, { user });
  return Object.assign(props, { unmount: view.unmount });
}

beforeEach(() => forgetSubtitleSearch());

describe("finding subtitles", () => {
  it("lists what the server finds in the profile's language, made for this file first, and adds the one picked", async () => {
    const requests = mockApi({
      "GET /api/items/7/subtitles/search": () => ({
        results: [result({}), result({ id: "opensubtitles:2", name: "Jaws.1975.DVDRip", matched: "title", hearing_impaired: true, downloads: 1 })],
      }),
      "POST /api/items/7/subtitles": () => added,
    });
    const props = renderMenu();
    await userEvent.click(screen.getByRole("button", { name: /Subtitles\s*None/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Find subtitles…" }));
    const rows = await screen.findAllByRole("menuitem");
    expect(rows.map((r) => r.textContent)).toEqual([
      "Jaws.1975.1080p.BluRay.x264-GRPEnglish · SRT · 1,234 downloadsMade for this file",
      "Jaws.1975.DVDRipEnglish · SRT · SDH · 1 download",
    ]);
    expect(requests[0]!.path).toBe("/api/items/7/subtitles/search?file_id=3&language=en");

    await userEvent.click(rows[0]!);
    await waitFor(() => expect(props.subtitleSources!.onAdded).toHaveBeenCalledWith(added));
    expect(requests[1]).toMatchObject({ method: "POST", body: { result: "opensubtitles:1", file_id: 3 } });
    expect(props.setOpen).toHaveBeenCalledWith(false);
  });

  it("searches again in another language", async () => {
    const requests = mockApi({ "GET /api/items/7/subtitles/search": () => ({ results: [] }) });
    renderMenu();
    await userEvent.click(screen.getByRole("button", { name: /Subtitles/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Find subtitles…" }));
    expect(await screen.findByText("No English subtitles found for this video.")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Subtitle language" }), "fre");
    expect(await screen.findByText("No French subtitles found for this video.")).toBeInTheDocument();
    expect(requests.map((r) => r.path)).toEqual([
      "/api/items/7/subtitles/search?file_id=3&language=en",
      "/api/items/7/subtitles/search?file_id=3&language=fr",
    ]);
    // Back goes to the subtitles, not the menu's top.
    await userEvent.click(screen.getByRole("button", { name: "Find subtitles" }));
    expect(screen.getByRole("menuitem", { name: "Find subtitles…" })).toBeInTheDocument();
  });

  it("says search isn't set up, pointing an administrator to Settings → Server, and leaves it out for others then", async () => {
    mockApi({
      "GET /api/items/7/subtitles/search": () =>
        new Response(JSON.stringify({ code: "unavailable", message: "no OpenSubtitles key", setup: false }), {
          status: 503,
          headers: { "content-type": "application/json" },
        }),
    });
    renderMenu();
    await userEvent.click(screen.getByRole("button", { name: /Subtitles/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Find subtitles…" }));
    expect(await screen.findByText("Subtitle search isn't set up on this server.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Set it up in Settings → Server" })).toHaveAttribute("href", "/settings/server");
  });

  it("is left out for a profile that can't set it up once the server said it isn't, and says so to one that can", async () => {
    mockApi({
      "GET /api/items/7/subtitles/search": () =>
        new Response(JSON.stringify({ code: "unavailable", message: "not set up", setup: false }), { status: 503, headers: { "content-type": "application/json" } }),
    });
    const first = renderMenu();
    await userEvent.click(screen.getByRole("button", { name: /Subtitles/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Find subtitles…" }));
    await screen.findByText("Subtitle search isn't set up on this server.");
    first.unmount();

    const other = renderMenu({}, {}, { ...testUser, is_admin: false });
    await userEvent.click(screen.getByRole("button", { name: /Subtitles/ }));
    expect(screen.queryByRole("menuitem", { name: /Find subtitles/ })).not.toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Add subtitle file…" })).toBeInTheDocument();
    other.unmount();

    renderMenu();
    await userEvent.click(screen.getByRole("button", { name: /Subtitles/ }));
    expect(screen.getByRole("menuitem", { name: /Find subtitles…\s*Subtitle search isn't set up/ })).toBeInTheDocument();
  });

  it("is left out on a server too old to search (404)", async () => {
    mockApi({ "GET /api/items/7/subtitles/search": () => apiError(404, "not_found", "no route") });
    const first = renderMenu();
    await userEvent.click(screen.getByRole("button", { name: /Subtitles/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Find subtitles…" }));
    expect(await screen.findByText("This server can't search for subtitles: it needs an update.")).toBeInTheDocument();
    first.unmount();
    renderMenu();
    await userEvent.click(screen.getByRole("button", { name: /Subtitles/ }));
    expect(screen.queryByRole("menuitem", { name: /Find subtitles/ })).not.toBeInTheDocument();
  });
});

describe("adding a subtitle file", () => {
  it("sends the file picked, in the language picked", async () => {
    const requests = mockApi({ "POST /api/items/7/subtitles": () => ({ ...added, language: "ger" }) });
    const props = renderMenu();
    await userEvent.click(screen.getByRole("button", { name: /Subtitles/ }));
    expect(screen.getByTestId("subtitle-file")).toHaveAttribute("accept", ".srt,.vtt,.ass,.ssa");
    const file = new File(["1\n00:00:01,000 --> 00:00:02,000\nHallo\n"], "Jaws.de.srt", { type: "application/x-subrip" });
    fireEvent.change(screen.getByTestId("subtitle-file"), { target: { files: [file] } });
    expect(await screen.findByText("Jaws.de.srt")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Language of the subtitles" }), "ger");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(props.subtitleSources!.onAdded).toHaveBeenCalled());
    const form = requests[0]!.init.body as FormData;
    expect(form).toBeInstanceOf(FormData);
    expect([(form.get("file") as File).name, form.get("language"), form.get("file_id")]).toEqual(["Jaws.de.srt", "ger", "3"]);
    // The browser sets the form's type, with its boundary.
    expect((requests[0]!.init.headers as Record<string, string>)["Content-Type"]).toBeUndefined();
  });

  it("refuses a file over 2 MB before sending it", async () => {
    const requests = mockApi({});
    const props = renderMenu();
    await userEvent.click(screen.getByRole("button", { name: /Subtitles/ }));
    const big = new File([new Uint8Array(2 * 1024 * 1024 + 1)], "big.srt");
    fireEvent.change(screen.getByTestId("subtitle-file"), { target: { files: [big] } });
    expect(props.onSubtitleFileRefused).toHaveBeenCalledWith("Subtitle files can be up to 2 MB.");
    expect(requests).toEqual([]);
  });
});

describe("added subtitles", () => {
  const stream: MediaStream = { index: 2000, kind: "subtitle", codec: "subrip", language: "eng", title: null, default: false, forced: false, text_based: true, external: true, added: true };

  it("can be deleted, after asking, by whoever may", async () => {
    mockApi({});
    const props = renderMenu({ subtitleStreams: [stream] });
    await userEvent.click(screen.getByRole("button", { name: /Subtitles/ }));
    expect(screen.getByRole("menuitemradio", { name: /English.*Added/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /^Delete English/ }));
    expect(screen.getByText("Delete for everyone?")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(props.onDeleteSubtitle).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: /^Delete English/ }));
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(props.onDeleteSubtitle).toHaveBeenCalledWith(2000);
  });

  it("have no delete for a profile that may not", async () => {
    mockApi({});
    renderMenu({ subtitleStreams: [stream, { ...stream, index: 5, added: false }], canDeleteSubtitle: () => false });
    await userEvent.click(screen.getByRole("button", { name: /Subtitles/ }));
    expect(screen.queryByRole("button", { name: /Delete/ })).not.toBeInTheDocument();
  });
});
