import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PlayingSession, RemoteDevice, TrackOption } from "@/api/types";
import { formatClock } from "@/lib/format";
import { playingHere } from "@/lib/playingHere";
import { openRemoteOn } from "@/lib/remote";
import { makeItem, makeMarker, makeReceiver, makeSession } from "@/test/fixtures";
import { apiError, mockApi, renderWithProviders, type FakeRequest } from "@/test/render";
import { RemoteButton } from "./Remote";

// A short wait for an answer, so the tests don't sit through the real one, yet long enough
// for a test to look at what shows while a command waits.
vi.mock("@/lib/remote", async (original) => ({ ...(await original<typeof import("@/lib/remote")>()), ANSWER_WAIT_MS: 200 }));

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

    const items = within(within(panel).getByRole("list")).getAllByRole("listitem");
    expect(items.map((i) => i.textContent)).toEqual([
      "Living room TVTest Show — S1 · E1 — Pilot",
      "Unknown device1 Channel One",
      "Den TVReady to play",
      "PhoneReady to play",
    ]);
    // Those that play are chosen; one ready to play has nothing to control.
    const rows = within(within(panel).getByRole("list")).getAllByRole("button");
    expect(rows.map((r) => r.getAttribute("aria-label"))).toEqual(["Living room TV", "Unknown device"]);
    expect(rows[0]).toHaveAccessibleDescription("Test Show — S1 · E1 — Pilot");
    expect(rows[1]).toHaveAccessibleDescription("1 Channel One");
    expect(within(items[3]!).queryByRole("button")).toBeNull();
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
    let sessions = [episode()];
    serve(
      () => sessions,
      () => [phone],
    );
    // Cast from a player, which hides the header and leaves.
    openRemoteOn("app-3");
    const { client, unmount } = renderWithProviders(<RemoteButton />);
    const remote = await screen.findByRole("dialog", { name: "Your devices" });
    // Its controls show once it plays, not the TV's meanwhile.
    expect(within(remote).queryByRole("group")).toBeNull();
    sessions = [episode(), episode({ session_id: "phone", device: "Phone", device_id: "app-3", device_kind: "phone" })];
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    expect(await within(remote).findByRole("group", { name: "Phone" })).toBeInTheDocument();
    expect(within(remote).getByRole("button", { name: "Phone" })).toHaveAttribute("aria-current", "true");
    unmount();

    renderWithProviders(<RemoteButton />);
    await screen.findByRole("button", { name: /^Remote/ });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("lists a device ready to play without offering to choose it", async () => {
    serve(
      () => [],
      () => [phone],
    );
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();
    expect(within(panel).getByRole("listitem")).toHaveTextContent("PhoneReady to play");
    expect(within(panel).queryByRole("button")).toBeNull();
    expect(within(panel).queryByRole("group")).toBeNull();
  });

  it("lists the household's devices on another profile after the user's own, by that profile, never what they play", async () => {
    const requests = serve(
      () => [episode()],
      () => [
        makeReceiver({ receiver_id: "attic", device_id: "app-6", name: "Attic TV", profile: { id: 2, name: "Sam" } }),
        phone,
        makeReceiver({ receiver_id: "tv-1", device_id: "app-1" }),
      ],
    );
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();
    // Asked for: without it, the list is the user's own devices alone.
    expect(requests.filter((r) => r.path.startsWith("/api/remote/devices")).map((r) => r.path)).toContain("/api/remote/devices?household=1");
    expect(within(within(panel).getByRole("list")).getAllByRole("listitem").map((i) => i.textContent)).toEqual([
      "Living room TVTest Show — S1 · E1 — Pilot",
      "PhoneReady to play",
      "Attic TVOn Sam's profile",
    ]);
    // Nothing to control there: only the TV playing is offered.
    expect(within(within(panel).getByRole("list")).getAllByRole("button").map((r) => r.getAttribute("aria-label"))).toEqual(["Living room TV"]);
  });

  it("shows how far in the chosen device's controls, its row leaving it out", async () => {
    serve(() => [episode(), episode({ session_id: "tv2", device: "Den TV", device_id: "app-5" })]);
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();
    const bars = (name: string) => within(panel).getByRole("button", { name }).querySelectorAll("[style]").length;
    expect([bars("Den TV"), bars("Living room TV")]).toEqual([1, 1]);

    await user.click(within(panel).getByRole("button", { name: "Den TV" }));
    expect([bars("Den TV"), bars("Living room TV")]).toEqual([0, 1]);
    expect(within(panel).getByRole("group", { name: "Den TV" })).toHaveTextContent("1:00");
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
      "Back 30 seconds on Living room TV",
      "Play on Living room TV",
      "Forward 30 seconds on Living room TV",
      "Next on Living room TV",
      "Stop on Living room TV",
    ]);
    expect(within(tv).getByRole("button", { name: "Next on Living room TV" })).toBeDisabled();
  });

  it("puts Previous at the left edge and Next at the right, the skips and play or pause between them", async () => {
    serve(() => [episode({ controls: ["previous", "next", "play", "pause", "skip"] })]);
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();
    const previous = within(panel).getByRole("button", { name: "Previous on Living room TV" });
    const row = previous.parentElement!.parentElement!;
    expect(Array.from(row.children).map((cell) => Array.from(cell.querySelectorAll("button")).map((b) => b.getAttribute("aria-label")))).toEqual([
      ["Previous on Living room TV"],
      ["Back 30 seconds on Living room TV", "Play on Living room TV", "Forward 30 seconds on Living room TV"],
      ["Next on Living room TV"],
    ]);
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
    // It shows playing at once, not when the player's report comes.
    expect(within(panel).getByRole("button", { name: "Pause on Living room TV" })).toBeInTheDocument();
    // The player's report brings `sessions_changed`.
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    await act(() => new Promise((r) => setTimeout(r, 100)));
    expect(within(panel).getByRole("button", { name: "Pause on Living room TV" })).toBeInTheDocument();
    expect(within(panel).queryByRole("alert")).toBeNull();
  });

  it("says so when the device didn't answer, and shows where it is again", async () => {
    serve(
      () => [episode({ controls: ["skip"] })],
      () => [],
      { "POST /api/sessions/tv/command": () => undefined },
    );
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Forward 30 seconds on Living room TV" }));
    expect(within(panel).getByRole("group")).toHaveTextContent("1:30");
    expect(await within(panel).findByRole("alert")).toHaveTextContent("Living room TV didn't answer.");
    expect(within(panel).getByRole("group")).toHaveTextContent("1:00");
  });

  it("goes where a seek or a skip sends it at once, a skip from where the last one went", async () => {
    let sessions = [episode({ controls: ["seek", "skip"] })];
    const requests = serve(
      () => sessions,
      () => [],
      { "POST /api/sessions/tv/command": () => undefined },
    );
    const { client } = renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();
    const tv = within(panel).getByRole("group");

    const seek = within(tv).getByRole("slider", { name: "Seek on Living room TV" });
    fireEvent.change(seek, { target: { value: "600000" } });
    fireEvent.pointerUp(seek);
    expect(tv).toHaveTextContent("10:00");
    await waitFor(() => expect(commands(requests)).toEqual([{ command: "seek", position_ms: 600_000 }]));
    await user.click(within(tv).getByRole("button", { name: "Forward 30 seconds on Living room TV" }));
    await user.click(within(tv).getByRole("button", { name: "Forward 30 seconds on Living room TV" }));
    expect(tv).toHaveTextContent("11:00");

    // The player reports it went there: it stays there, with no failure.
    sessions = [episode({ controls: ["seek", "skip"], position_ms: 660_000, updated_at: nowSecs() + 1 })];
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    await act(() => new Promise((r) => setTimeout(r, 100)));
    expect(tv).toHaveTextContent("11:00");
    expect(within(panel).queryByRole("alert")).toBeNull();
  });

  it("goes back 30 seconds", async () => {
    const requests = serve(
      () => [episode({ controls: ["skip"], position_ms: 600_000 })],
      () => [],
      { "POST /api/sessions/tv/command": () => undefined },
    );
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Back 30 seconds on Living room TV" }));
    expect(commands(requests)).toEqual([{ command: "skip", offset_ms: -30_000 }]);
    expect(within(panel).getByRole("group")).toHaveTextContent("9:30");
  });

  describe("Skip Intro and Skip Credits", () => {
    const intro = makeMarker({ kind: "intro", start_ms: 10_000, end_ms: 70_000 });
    const credits = makeMarker({ kind: "credits", start_ms: 1_700_000, end_ms: 1_790_000 });
    /** The episode, paused where `overrides` say, its player seeking. */
    const withMarkers = (overrides: Partial<PlayingSession> = {}) =>
      episode({ controls: ["seek", "skip"], markers: [intro, credits], ...overrides });
    const offer = (panel: HTMLElement) => within(panel).queryByRole("button", { name: /^Skip/ });

    it("offers to skip an intro or the credits while in them, until a second before they end", async () => {
      let position = 9_999;
      serve(() => [withMarkers({ position_ms: position })]);
      const { client } = renderWithProviders(<RemoteButton />);
      const { panel } = await openRemote();
      const seek = within(panel).getByRole("slider", { name: "Seek on Living room TV" });
      const at = async (ms: number) => {
        position = ms;
        await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
        // Once the remote shows it there.
        await waitFor(() => expect(seek).toHaveAttribute("aria-valuetext", `${formatClock(ms / 1000)} of 30:00`));
        return offer(panel);
      };

      expect(offer(panel)).toBeNull();
      const skipIntro = await at(10_000);
      expect(skipIntro).toHaveAccessibleName("Skip intro on Living room TV");
      expect(skipIntro).toHaveTextContent("Skip Intro");
      expect(await at(68_999)).toHaveAccessibleName("Skip intro on Living room TV");
      expect(await at(69_000)).toBeNull();
      expect(await at(600_000)).toBeNull();
      const skipCredits = await at(1_700_000);
      expect(skipCredits).toHaveAccessibleName("Skip credits on Living room TV");
      expect(skipCredits).toHaveTextContent("Skip Credits");
      expect(await at(1_788_999)).not.toBeNull();
      expect(await at(1_789_000)).toBeNull();
    });

    it("skips an intro to its last moment, and goes once there", async () => {
      const requests = serve(
        () => [withMarkers({ position_ms: 30_000 })],
        () => [],
        { "POST /api/sessions/tv/command": () => undefined },
      );
      renderWithProviders(<RemoteButton />);
      const { user, panel } = await openRemote();

      await user.click(within(panel).getByRole("button", { name: "Skip intro on Living room TV" }));
      expect(commands(requests)).toEqual([{ command: "seek", position_ms: 69_250 }]);
      // The bar shows it there at once, past the offer.
      expect(within(panel).getByRole("group")).toHaveTextContent("1:09");
      expect(offer(panel)).toBeNull();
    });

    it("skips the credits to their end, and no further than the title's", async () => {
      let markers = [credits];
      const requests = serve(
        () => [withMarkers({ position_ms: 1_750_000, markers })],
        () => [],
        { "POST /api/sessions/tv/command": () => undefined },
      );
      const { client } = renderWithProviders(<RemoteButton />);
      const { user, panel } = await openRemote();

      await user.click(within(panel).getByRole("button", { name: "Skip credits on Living room TV" }));
      expect(commands(requests)).toEqual([{ command: "seek", position_ms: 1_790_000 }]);

      // Credits said to run on past the end the player knows (its length is 30:00).
      markers = [makeMarker({ kind: "credits", start_ms: 1_700_000, end_ms: 1_800_500 })];
      await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
      await user.click(await within(panel).findByRole("button", { name: "Skip credits on Living room TV" }));
      expect(commands(requests).at(-1)).toEqual({ command: "seek", position_ms: 1_800_000 });
    });

    it("offers no skip where the player doesn't seek, nor on Live TV", async () => {
      let sessions = [withMarkers({ position_ms: 30_000 })];
      serve(() => sessions);
      const { client } = renderWithProviders(<RemoteButton />);
      const { panel } = await openRemote();
      expect(offer(panel)).not.toBeNull();

      sessions = [withMarkers({ position_ms: 30_000, controls: ["play", "pause", "skip"] })];
      await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
      await waitFor(() => expect(within(panel).getByRole("button", { name: "Play on Living room TV" })).toBeInTheDocument());
      expect(offer(panel)).toBeNull();

      // A channel its player can go back in, with a marker where it is.
      sessions = [live({ controls: ["seek", "skip", "next", "previous"], markers: [makeMarker({ start_ms: 0, end_ms: 600_000 })] })];
      await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
      expect(await within(panel).findByRole("group", { name: "Unknown device" })).toBeInTheDocument();
      expect(offer(panel)).toBeNull();
    });

    it("keeps the offer whole at the end of the header, a long title beside it giving way", async () => {
      const film = makeItem({ id: 9, kind: "movie", title: "A Title Long Enough to Take Several Lines in the Panel's Header" });
      serve(() => [withMarkers({ item: film, position_ms: 30_000 })]);
      renderWithProviders(<RemoteButton />);
      const { panel } = await openRemote();

      const skip = within(panel).getByRole("button", { name: "Skip intro on Living room TV" });
      const title = within(within(panel).getByRole("group")).getByText(film.title);
      const header = skip.parentElement!;
      expect(header).toContainElement(title);
      expect(header.lastElementChild).toBe(skip);
      // One line at its own width; the title takes what is left, two lines at most, cut with an ellipsis.
      expect(skip).toHaveClass("shrink-0", "whitespace-nowrap");
      expect(title).toHaveClass("line-clamp-2");
      expect(title.parentElement).toHaveClass("min-w-0", "flex-1");
    });
  });

  it("keeps what played between a next and the next one, on a device that only plays too", async () => {
    let sessions = [episode({ controls: ["next"] })];
    serve(
      () => sessions,
      () => [],
      {
        // Next ends the session: the next one is listed a moment later.
        "POST /api/sessions/tv/command": () => {
          sessions = [];
          return undefined;
        },
      },
    );
    const { client } = renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Next on Living room TV" }));
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    expect(within(panel).getByRole("group", { name: "Living room TV" })).toHaveTextContent("Pilot");
    expect(within(panel).getByRole("button", { name: "Living room TV" })).toHaveAccessibleDescription("Test Show — S1 · E1 — Pilot");

    const second = makeItem({ id: 8, kind: "episode", title: "Second", grandparent_title: "Test Show", parent_index: 1, index: 2 });
    sessions = [episode({ session_id: "tv2", item: second, controls: ["next"] })];
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    expect(await within(panel).findByText("Second")).toBeInTheDocument();
    expect(within(panel).getByRole("status")).toBeEmptyDOMElement();
  });

  it("lets a device go once a next started nothing", async () => {
    let sessions = [episode({ controls: ["next"] })];
    serve(
      () => sessions,
      () => [],
      {
        "POST /api/sessions/tv/command": () => {
          sessions = [];
          return undefined;
        },
      },
    );
    const { client } = renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();
    await user.click(within(panel).getByRole("button", { name: "Next on Living room TV" }));
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    expect(within(panel).getByRole("group", { name: "Living room TV" })).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("button", { name: /^Remote/ })).toBeNull());
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

    it("keeps what plays while a track picked starts a new session", async () => {
      let sessions = [withTracks()];
      serve(
        () => sessions,
        () => [],
        {
          "POST /api/sessions/tv/command": () => {
            sessions = [];
            return undefined;
          },
        },
      );
      const { client } = renderWithProviders(<RemoteButton />);
      const { user, panel } = await openRemote();

      await user.click(within(panel).getByRole("button", { name: "Subtitles on Living room TV" }));
      await user.click(within(panel).getByRole("menuitemradio", { name: "French (SRT, Forced)" }));
      await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
      expect(within(panel).getByRole("group", { name: "Living room TV" })).toBeInTheDocument();
      sessions = [withTracks({ session_id: "tv2", subtitle_index: 3 })];
      await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
      await waitFor(() => expect(within(panel).getByRole("status")).toBeEmptyDOMElement());
      expect(within(panel).queryByRole("alert")).toBeNull();
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

  it("asks before it stops, and Cancel or Escape goes back to Stop", async () => {
    const requests = serve(() => [episode()]);
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Stop on Living room TV" }));
    const ask = within(panel).getByRole("alertdialog", { name: "Stop playing on Living room TV?" });
    expect(within(ask).getByRole("button", { name: "Stop" })).toHaveFocus();
    await user.click(within(ask).getByRole("button", { name: "Cancel" }));
    expect(within(panel).queryByRole("alertdialog")).toBeNull();
    await waitFor(() => expect(within(panel).getByRole("button", { name: "Stop on Living room TV" })).toHaveFocus());

    await user.click(within(panel).getByRole("button", { name: "Stop on Living room TV" }));
    await user.keyboard("{Escape}");
    expect(within(panel).queryByRole("alertdialog")).toBeNull();
    expect(screen.getByRole("dialog", { name: "Your devices" })).toBeInTheDocument();
    expect(requests.some((r) => r.method === "DELETE")).toBe(false);
  });

  it("picks the quality from those the player's menu offers, the one playing ticked", async () => {
    let quality = "auto";
    let controls: PlayingSession["controls"] = ["quality"];
    const qualities = [
      { id: "original", label: "Original" },
      { id: "auto", label: "Automatic" },
      { id: "8m", label: "8 Mbps 1080p" },
    ];
    const requests = serve(
      () => [episode({ controls, qualities, quality })],
      () => [],
      {
        "POST /api/sessions/tv/command": (req) => {
          quality = (req.body as { quality: string }).quality;
          return undefined;
        },
      },
    );
    const { client } = renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Quality on Living room TV" }));
    const menu = within(panel).getByRole("menu", { name: "Quality on Living room TV" });
    expect(within(menu).getAllByRole("menuitemradio").map((o) => [o.textContent, o.getAttribute("aria-checked")])).toEqual([
      ["Original", "false"],
      ["Automatic", "true"],
      ["8 Mbps 1080p", "false"],
    ]);
    await user.click(within(menu).getByRole("menuitemradio", { name: "8 Mbps 1080p" }));
    expect(commands(requests)).toEqual([{ command: "quality", quality: "8m" }]);
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    await waitFor(() => expect(within(panel).getByRole("status")).toBeEmptyDOMElement());
    expect(within(panel).queryByRole("alert")).toBeNull();

    // A player that doesn't take it has no button.
    controls = [];
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    await waitFor(() => expect(within(panel).queryByRole("button", { name: "Quality on Living room TV" })).toBeNull());
  });

  it("shows a channel's programme, how far along it is and what's next, with channel buttons", async () => {
    const start = nowSecs() - 15 * 60;
    const channel = { id: 3, name: "Channel One", number: "1", logo: null, now: "Evening News", now_start: start, now_stop: start + 3_600, next: "Late Show" };
    serve(() => [live({ channel, controls: ["next", "previous", "skip"] })]);
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();

    const box = within(panel).getByRole("group", { name: "Unknown device" });
    expect(box).toHaveTextContent("Evening News");
    expect(box).toHaveTextContent("Next: Late Show");
    expect(within(box).getByRole("progressbar")).toBeInTheDocument();
    expect(within(box).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual([
      "Previous channel on Unknown device",
      "Next channel on Unknown device",
      "Stop on Unknown device",
    ]);
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
    const ask = within(panel).getByRole("alertdialog", { name: "Stop playing on Living room TV?" });
    await user.click(within(ask).getByRole("button", { name: "Stop" }));
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/sessions/tv")).toBe(true);
    await waitFor(() => expect(within(panel).queryByRole("button", { name: "Living room TV" })).toBeNull());
    // The phone is all that's left, ready to play.
    expect(within(panel).getByRole("listitem")).toHaveTextContent("PhoneReady to play");
    expect(within(panel).queryByRole("group")).toBeNull();
  });
});
