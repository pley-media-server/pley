import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PlayingSession, RemoteDevice, TrackOption } from "@/api/types";
import { playingHere } from "@/lib/playingHere";
import { openRemoteOn } from "@/lib/remote";
import { makeItem, makeReceiver, makeSession } from "@/test/fixtures";
import { apiError, mockApi, renderWithProviders, type FakeRequest } from "@/test/render";
import { RemoteButton } from "./Remote";

// A short wait for an answer, so the tests don't sit through the real one.
vi.mock("@/lib/remote", async (original) => ({ ...(await original<typeof import("@/lib/remote")>()), ANSWER_WAIT_MS: 50 }));

const nowSecs = () => Math.floor(Date.now() / 1000);

/** An episode on the living room TV, paused a minute in. */
function episode(overrides: Partial<PlayingSession> = {}): PlayingSession {
  return makeSession({
    session_id: "tv",
    device: "Living room TV",
    device_id: "app-1",
    item: makeItem({
      id: 7,
      kind: "episode",
      title: "Pilot",
      grandparent_title: "Test Show",
      parent_index: 1,
      index: 1,
      duration_ms: 1_800_000,
    }),
    state: "paused",
    position_ms: 60_000,
    duration_ms: 1_800_000,
    updated_at: nowSecs(),
    ...overrides,
  });
}

/** A channel watched on a device the server couldn't name. */
function live(overrides: Partial<PlayingSession> = {}): PlayingSession {
  return makeSession({
    session_id: "box",
    kind: "live",
    device: null,
    device_id: "app-2",
    device_kind: "other",
    item: null,
    channel: { id: 3, name: "Channel One", number: "1", logo: null, now: "Evening News" },
    position_ms: 0,
    duration_ms: null,
    updated_at: nowSecs(),
    controls: ["next", "previous"],
    ...overrides,
  });
}

/** A phone with the app open, playing nothing. */
const phone: RemoteDevice = makeReceiver({ receiver_id: "phone-1", device_id: "app-3", name: "Phone", kind: "phone" });

/** The server: the user's sessions and devices as they are when asked. */
function serve(sessions: () => PlayingSession[], receivers: () => RemoteDevice[] = () => [], more: Parameters<typeof mockApi>[0] = {}) {
  return mockApi({ "GET /api/sessions": sessions, "GET /api/remote/devices": receivers, ...more });
}

let leave: (() => void) | null = null;
afterEach(() => {
  leave?.();
  leave = null;
  vi.useRealTimers();
});

async function openRemote() {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: /^Remote/ }));
  return { user, panel: screen.getByRole("dialog", { name: "Your devices" }) };
}

const commands = (requests: FakeRequest[]) => requests.filter((r) => r.method === "POST").map((r) => r.body);

describe("RemoteButton", () => {
  it("shows whenever another device is on, playing or ready to play", async () => {
    let sessions: PlayingSession[] = [];
    let receivers: RemoteDevice[] = [];
    serve(
      () => sessions,
      () => receivers,
    );
    const { client } = renderWithProviders(<RemoteButton />);
    await waitFor(() => expect(client.getQueryData(["remote", "devices"])).toEqual([]));
    expect(screen.queryByRole("button", { name: /^Remote/ })).toBeNull();

    // This tab's own session isn't another device.
    leave = playingHere("tv");
    sessions = [episode()];
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    expect(screen.queryByRole("button", { name: /^Remote/ })).toBeNull();

    receivers = [phone];
    await act(() => client.invalidateQueries({ queryKey: ["remote", "devices"] }));
    expect(await screen.findByRole("button", { name: "Remote, 1 device ready to play" })).toBeInTheDocument();

    sessions = [episode(), live()];
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    expect(await screen.findByRole("button", { name: "Remote, 1 device playing" })).toBeInTheDocument();
  });

  it("lists the devices that play first, with what they play, then those ready to play, each by name", async () => {
    serve(
      () => [episode(), live()],
      () => [makeReceiver({ receiver_id: "tv-1", device_id: "app-1" }), phone, makeReceiver({ receiver_id: "den", device_id: "app-4", name: "Den TV" })],
    );
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();

    const rows = within(within(panel).getByRole("list")).getAllByRole("button");
    expect(rows.map((r) => r.getAttribute("aria-label"))).toEqual(["Living room TV", "Unknown device", "Den TV", "Phone"]);
    expect(rows[0]).toHaveAccessibleDescription("Test Show — S1 · E1 — Pilot");
    expect(rows[1]).toHaveAccessibleDescription("1 Channel One");
    expect(rows[3]).toHaveAccessibleDescription("Ready to play");
  });

  it("numbers devices of the same name", async () => {
    serve(() => [episode(), episode({ session_id: "tv2", device_id: "app-5" }), live()]);
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();
    const rows = within(within(panel).getByRole("list")).getAllByRole("button");
    expect(rows.map((r) => r.getAttribute("aria-label"))).toEqual(["Living room TV (1)", "Living room TV (2)", "Unknown device"]);

    await user.click(rows[1]!);
    expect(within(panel).getByRole("button", { name: "Stop on Living room TV (2)" })).toBeInTheDocument();
  });

  it("shows a device's controls below the list once it is chosen, while the remote is open", async () => {
    serve(() => [episode(), live()]);
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();
    expect(within(panel).queryByRole("group")).toBeNull();

    await user.click(within(panel).getByRole("button", { name: "Unknown device" }));
    expect(within(panel).getByRole("button", { name: "Unknown device" })).toHaveAttribute("aria-current", "true");
    expect(within(panel).getByRole("group", { name: "Unknown device" })).toHaveTextContent("1 Channel One");
    expect(within(panel).getByRole("group", { name: "Unknown device" })).toHaveTextContent("Evening News");
    await user.click(within(panel).getByRole("button", { name: "Living room TV" }));
    expect(within(panel).getByRole("group", { name: "Living room TV" })).toBeInTheDocument();
    expect(within(panel).queryByRole("group", { name: "Unknown device" })).toBeNull();

    // Closed and opened again, it asks again.
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: /^Remote/ }));
    expect(within(screen.getByRole("dialog")).queryByRole("group")).toBeNull();
  });

  it("opens on the device a page cast to, once the header shows, and once only", async () => {
    serve(
      () => [episode()],
      () => [phone],
    );
    // Cast from a player, which hides the header and leaves.
    openRemoteOn("app-3");
    const { unmount } = renderWithProviders(<RemoteButton />);
    const remote = await screen.findByRole("dialog", { name: "Your devices" });
    expect(within(remote).getByRole("button", { name: "Phone" })).toHaveAttribute("aria-current", "true");
    expect(within(remote).getByRole("group", { name: "Phone" })).toBeInTheDocument();
    unmount();

    renderWithProviders(<RemoteButton />);
    await screen.findByRole("button", { name: /^Remote/ });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("says how to send something to a device ready to play", async () => {
    serve(
      () => [],
      () => [phone],
    );
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();
    const idle = within(panel).getByRole("group", { name: "Phone" });
    expect(idle).toHaveTextContent("Nothing playing");
    expect(idle).toHaveTextContent("Find something to watch or listen to, then choose Play on… and this device.");
    expect(within(idle).queryByRole("button")).toBeNull();
  });

  it("offers the controls the player obeys, and Next greyed out where it doesn't take it", async () => {
    serve(() => [episode({ controls: ["play", "pause", "seek", "skip"] })]);
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();

    const tv = within(panel).getByRole("group", { name: "Living room TV" });
    expect(tv).toHaveTextContent("Test Show · S1 · E1");
    expect(tv).toHaveTextContent("Pilot");
    expect(within(tv).getByRole("slider", { name: "Seek on Living room TV" })).toBeInTheDocument();
    expect(within(tv).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual([
      "Back 10 seconds on Living room TV",
      "Play on Living room TV",
      "Forward 30 seconds on Living room TV",
      "Next on Living room TV",
      "Stop on Living room TV",
    ]);
    expect(within(tv).getByRole("button", { name: "Next on Living room TV" })).toBeDisabled();
  });

  it("changes channel on Live TV, which has no place to show", async () => {
    serve(() => [live()]);
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();
    const box = within(panel).getByRole("group", { name: "Unknown device" });
    expect(within(box).queryByRole("slider")).toBeNull();
    expect(within(box).queryByRole("progressbar")).toBeNull();
    expect(within(box).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual([
      "Previous channel on Unknown device",
      "Next channel on Unknown device",
      "Stop on Unknown device",
    ]);
  });

  it("leaves the title off a poster too small for it, the line beside it saying it", async () => {
    const film = makeItem({ id: 9, kind: "movie", title: "Test Pattern", poster_url: null });
    serve(() => [episode({ item: film })]);
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();
    expect(within(within(panel).getByRole("group")).getAllByText(/Test Pattern/)).toHaveLength(1);
  });

  it("shows a movie whose player obeys nothing with its progress and Stop", async () => {
    serve(() => [episode({ item: makeItem({ id: 9, kind: "movie", title: "Test Pattern" }) })]);
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();
    const tv = within(panel).getByRole("group");
    expect(within(tv).getByRole("progressbar", { name: "Progress on Living room TV" })).toBeInTheDocument();
    expect(within(tv).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual(["Stop on Living room TV"]);
  });

  it("moves the place on each second while it plays", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    // Reported a minute before the answer came, by a server clock ten minutes ahead of this one.
    serve(() => [episode({ state: "playing", updated_at: nowSecs() + 600, age_ms: 60_000 })]);
    renderWithProviders(<RemoteButton />);
    fireEvent.click(await screen.findByRole("button", { name: /^Remote/ }));
    const panel = screen.getByRole("group");
    expect(panel).toHaveTextContent(/2:0[01]/);
    act(() => {
      vi.advanceTimersByTime(5_000);
    });
    expect(panel).toHaveTextContent(/2:0[56]/);
    expect(panel).toHaveTextContent("30:00");
  });

  it("sends a command, and the list shows it was taken", async () => {
    let sessions = [episode({ controls: ["play", "pause"] })];
    const requests = serve(
      () => sessions,
      () => [],
      {
        "POST /api/sessions/tv/command": () => {
          sessions = [episode({ controls: ["play", "pause"], state: "playing", updated_at: nowSecs() })];
          return undefined;
        },
      },
    );
    const { client } = renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Play on Living room TV" }));
    expect(commands(requests)).toEqual([{ command: "play" }]);
    // The player's report brings `sessions_changed`.
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    expect(await within(panel).findByRole("button", { name: "Pause on Living room TV" })).toBeInTheDocument();
    await act(() => new Promise((r) => setTimeout(r, 100)));
    expect(within(panel).queryByRole("alert")).toBeNull();
  });

  it("says so when the device didn't answer", async () => {
    serve(
      () => [episode({ controls: ["skip"] })],
      () => [],
      { "POST /api/sessions/tv/command": () => undefined },
    );
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Forward 30 seconds on Living room TV" }));
    expect(await within(panel).findByRole("alert")).toHaveTextContent("Living room TV didn't answer.");
  });

  it("doesn't call a next or previous that changed nothing a failure", async () => {
    serve(
      () => [episode({ controls: ["next"] })],
      () => [],
      { "POST /api/sessions/tv/command": () => undefined },
    );
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Next on Living room TV" }));
    expect(within(panel).getByRole("status")).toHaveTextContent("Waiting for Living room TV");
    await waitFor(() => expect(within(panel).getByRole("status")).toBeEmptyDOMElement());
    expect(within(panel).queryByRole("alert")).toBeNull();
  });

  describe("tracks", () => {
    const tracks: { audio: TrackOption[]; subtitles: TrackOption[] } = {
      audio: [
        { index: 1, language: "eng", title: null, codec: "truehd", channels: 8, default: true, forced: false },
        { index: 2, language: "eng", title: "Commentary", codec: "ac3", channels: 2, default: false, forced: false },
      ],
      subtitles: [{ index: 3, language: "fre", title: null, codec: "subrip", channels: null, default: false, forced: true }],
    };
    const withTracks = (overrides: Partial<PlayingSession> = {}) =>
      episode({ controls: ["audio", "subtitles"], tracks, audio_index: 1, subtitle_index: -1, ...overrides });

    it("picks the audio and the subtitles, named as the player's menu names them, the ones playing ticked", async () => {
      // What the player plays: it takes what it is sent at once.
      let playing: Partial<PlayingSession> = { audio_index: 1, subtitle_index: -1 };
      const requests = serve(
        () => [withTracks(playing)],
        () => [],
        {
          "POST /api/sessions/tv/command": (req) => {
            const { command, stream_index } = req.body as { command: string; stream_index: number };
            playing = { ...playing, [command === "audio" ? "audio_index" : "subtitle_index"]: stream_index };
            return undefined;
          },
        },
      );
      const { client } = renderWithProviders(<RemoteButton />);
      const { user, panel } = await openRemote();

      await user.click(within(panel).getByRole("button", { name: "Audio on Living room TV" }));
      const audio = within(panel).getByRole("menu", { name: "Audio on Living room TV" });
      expect(within(audio).getAllByRole("menuitemradio").map((o) => [o.textContent, o.getAttribute("aria-checked")])).toEqual([
        ["English (TrueHD 7.1)", "true"],
        ["English – Commentary (AC3 Stereo)", "false"],
      ]);
      expect(within(audio).getByRole("menuitemradio", { name: "English (TrueHD 7.1)" })).toHaveFocus();
      await user.click(within(audio).getByRole("menuitemradio", { name: "English – Commentary (AC3 Stereo)" }));
      expect(within(panel).queryByRole("menu")).toBeNull();
      expect(within(panel).getByRole("button", { name: "Audio on Living room TV" })).toHaveFocus();
      expect(within(panel).getByRole("status")).toHaveTextContent("Waiting for Living room TV");
      await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
      await waitFor(() => expect(within(panel).getByRole("status")).toBeEmptyDOMElement());

      await user.click(within(panel).getByRole("button", { name: "Subtitles on Living room TV" }));
      const subtitles = within(panel).getByRole("menu", { name: "Subtitles on Living room TV" });
      expect(within(subtitles).getAllByRole("menuitemradio").map((o) => [o.textContent, o.getAttribute("aria-checked")])).toEqual([
        ["Off", "true"],
        ["French (SRT, Forced)", "false"],
      ]);
      await user.click(within(subtitles).getByRole("menuitemradio", { name: "French (SRT, Forced)" }));

      expect(commands(requests)).toEqual([
        { command: "audio", stream_index: 2 },
        { command: "subtitles", stream_index: 3 },
      ]);
      await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
      await act(() => new Promise((r) => setTimeout(r, 100)));
      expect(within(panel).queryByRole("alert")).toBeNull();

      // Opened again, the subtitles now playing are ticked; Escape closes the menu alone.
      await user.click(within(panel).getByRole("button", { name: "Subtitles on Living room TV" }));
      expect(within(panel).getByRole("menuitemradio", { name: "French (SRT, Forced)" })).toHaveAttribute("aria-checked", "true");
      await user.keyboard("{Escape}");
      expect(within(panel).queryByRole("menu")).toBeNull();
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });

    it("says so when the device didn't take the track picked", async () => {
      serve(
        () => [withTracks()],
        () => [],
        { "POST /api/sessions/tv/command": () => undefined },
      );
      renderWithProviders(<RemoteButton />);
      const { user, panel } = await openRemote();

      await user.click(within(panel).getByRole("button", { name: "Subtitles on Living room TV" }));
      await user.click(within(panel).getByRole("menuitemradio", { name: "French (SRT, Forced)" }));
      expect(await within(panel).findByRole("alert")).toHaveTextContent("Living room TV didn't answer.");
    });

    it("offers audio where there is a choice, and neither where the player doesn't take them", async () => {
      let sessions = [withTracks({ tracks: { ...tracks, audio: tracks.audio.slice(0, 1) } })];
      serve(() => sessions);
      const { client } = renderWithProviders(<RemoteButton />);
      const { panel } = await openRemote();
      expect(within(panel).queryByRole("button", { name: "Audio on Living room TV" })).toBeNull();
      expect(within(panel).getByRole("button", { name: "Subtitles on Living room TV" })).toBeInTheDocument();

      sessions = [withTracks({ controls: [] })];
      await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
      await waitFor(() => expect(within(panel).queryByRole("button", { name: "Subtitles on Living room TV" })).toBeNull());
      expect(within(panel).queryByRole("button", { name: "Audio on Living room TV" })).toBeNull();
    });
  });

  it("gives focus back to the button on Escape, and opens closed when a device is on again", async () => {
    let sessions = [episode()];
    serve(() => sessions);
    const { client } = renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();
    within(panel).getByRole("button", { name: "Stop on Living room TV" }).focus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("button", { name: /^Remote/ })).toHaveFocus();

    await user.click(screen.getByRole("button", { name: /^Remote/ }));
    sessions = [];
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    await waitFor(() => expect(screen.queryByRole("button", { name: /^Remote/ })).toBeNull());
    sessions = [episode()];
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    expect(await screen.findByRole("button", { name: /^Remote/ })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("says why a command wasn't taken", async () => {
    serve(
      () => [episode({ controls: ["next"] })],
      () => [],
      { "POST /api/sessions/tv/command": () => apiError(429, "too_many_requests", "slow down") },
    );
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Next on Living room TV" }));
    expect(await within(panel).findByRole("alert")).toHaveTextContent("Too many at once: try again in a moment.");
  });

  it("stops a device: one ready to play stays, one that only played goes", async () => {
    let sessions = [episode()];
    const requests = serve(
      () => sessions,
      () => [phone],
      {
        "DELETE /api/sessions/tv": () => {
          sessions = [];
          return undefined;
        },
      },
    );
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Living room TV" }));
    await user.click(within(panel).getByRole("button", { name: "Stop on Living room TV" }));
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/sessions/tv")).toBe(true);
    await waitFor(() => expect(within(panel).queryByRole("button", { name: "Living room TV" })).toBeNull());
    // The phone is all that's left, and shown.
    expect(within(panel).getByRole("group", { name: "Phone" })).toHaveTextContent("Nothing playing");
  });
});
