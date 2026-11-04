import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PlayingSession } from "@/api/types";
import { playingHere } from "@/lib/playingHere";
import { makeItem } from "@/test/fixtures";
import { apiError, mockApi, renderWithProviders } from "@/test/render";
import { RemoteButton } from "./Remote";

// A short wait for an answer, so the tests don't sit through the real one.
vi.mock("@/lib/remote", async (original) => ({ ...(await original<typeof import("@/lib/remote")>()), ANSWER_WAIT_MS: 50 }));

const nowSecs = () => Math.floor(Date.now() / 1000);

function episode(overrides: Partial<PlayingSession> = {}): PlayingSession {
  return {
    session_id: "tv",
    kind: "item",
    device: "Living room TV",
    item: makeItem({
      id: 7,
      kind: "episode",
      title: "Pilot",
      grandparent_title: "The Office",
      parent_index: 1,
      index: 1,
      duration_ms: 1_800_000,
    }),
    channel: null,
    state: "paused",
    position_ms: 60_000,
    duration_ms: 1_800_000,
    updated_at: nowSecs(),
    controls: [],
    ...overrides,
  };
}

const live: PlayingSession = {
  session_id: "box",
  kind: "live",
  device: null,
  item: null,
  channel: { id: 3, name: "BBC One", number: "1", logo: null, now: "The News" },
  state: "playing",
  position_ms: 0,
  duration_ms: null,
  updated_at: nowSecs(),
  controls: ["next", "previous"],
};

let leave: (() => void) | null = null;
afterEach(() => {
  leave?.();
  leave = null;
  vi.useRealTimers();
});

async function openRemote() {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: /^Remote/ }));
  return { user, panel: screen.getByRole("dialog", { name: "Playing on your devices" }) };
}

describe("RemoteButton", () => {
  it("shows only while another device plays", async () => {
    let sessions: PlayingSession[] = [];
    mockApi({ "GET /api/sessions": () => sessions });
    const { client } = renderWithProviders(<RemoteButton />);
    await waitFor(() => expect(client.getQueryData(["sessions"])).toEqual([]));
    expect(screen.queryByRole("button", { name: /^Remote/ })).toBeNull();

    // This tab's own session isn't another device.
    leave = playingHere("tv");
    sessions = [episode()];
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    expect(screen.queryByRole("button", { name: /^Remote/ })).toBeNull();

    sessions = [episode(), live];
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    expect(await screen.findByRole("button", { name: "Remote, 1 device playing" })).toBeInTheDocument();
  });

  it("lists each device with what plays there and only the controls it obeys", async () => {
    mockApi({ "GET /api/sessions": () => [episode({ controls: ["play", "pause", "seek", "skip", "next"] }), live] });
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();

    const tv = within(panel).getByRole("group", { name: "Living room TV" });
    const box = within(panel).getByRole("group", { name: "Unknown device" });
    expect(tv).toHaveTextContent("The Office — S1 · E1 — Pilot");
    expect(within(tv).getByRole("slider", { name: "Seek on Living room TV" })).toBeInTheDocument();
    for (const name of ["Play", "Back 10 seconds", "Forward 30 seconds", "Next", "Stop"]) {
      expect(within(tv).getByRole("button", { name: `${name} on Living room TV` })).toBeInTheDocument();
    }
    expect(within(tv).queryByRole("button", { name: /^Pause|^Previous/ })).toBeNull();

    expect(box).toHaveTextContent("BBC One");
    expect(box).toHaveTextContent("The News");
    expect(within(box).queryByRole("slider")).toBeNull();
    expect(within(box).queryByRole("progressbar")).toBeNull();
    expect(within(box).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual([
      "Previous channel on Unknown device",
      "Next channel on Unknown device",
      "Stop on Unknown device",
    ]);
  });

  it("shows a player that obeys nothing with its progress and Stop", async () => {
    mockApi({ "GET /api/sessions": () => [episode()] });
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();
    expect(within(panel).getByRole("progressbar", { name: "Progress on Living room TV" })).toBeInTheDocument();
    expect(within(panel).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual(["Stop on Living room TV"]);
  });

  it("moves the place on each second while it plays", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    mockApi({ "GET /api/sessions": () => [episode({ state: "playing", updated_at: nowSecs() - 60 })] });
    renderWithProviders(<RemoteButton />);
    fireEvent.click(await screen.findByRole("button", { name: /^Remote/ }));
    const panel = screen.getByRole("dialog");
    expect(panel).toHaveTextContent(/2:0[01]/);
    act(() => {
      vi.advanceTimersByTime(5_000);
    });
    expect(panel).toHaveTextContent(/2:0[56]/);
    expect(panel).toHaveTextContent("30:00");
  });

  it("sends a command, and the list shows it was taken", async () => {
    let sessions = [episode({ controls: ["play", "pause"] })];
    const requests = mockApi({
      "GET /api/sessions": () => sessions,
      "POST /api/sessions/tv/command": () => {
        sessions = [episode({ controls: ["play", "pause"], state: "playing", updated_at: nowSecs() })];
        return undefined;
      },
    });
    const { client } = renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Play on Living room TV" }));
    expect(requests.find((r) => r.method === "POST")?.body).toEqual({ command: "play" });
    // The player's report brings `sessions_changed`.
    await act(() => client.invalidateQueries({ queryKey: ["sessions"] }));
    expect(await within(panel).findByRole("button", { name: "Pause on Living room TV" })).toBeInTheDocument();
    await act(() => new Promise((r) => setTimeout(r, 100)));
    expect(within(panel).queryByRole("alert")).toBeNull();
  });

  it("says so when the device didn't answer", async () => {
    mockApi({
      "GET /api/sessions": () => [episode({ controls: ["skip"] })],
      "POST /api/sessions/tv/command": () => undefined,
    });
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Forward 30 seconds on Living room TV" }));
    expect(await within(panel).findByRole("alert")).toHaveTextContent("Living room TV didn't answer.");
  });

  it("doesn't call a next or previous that changed nothing a failure", async () => {
    mockApi({
      "GET /api/sessions": () => [episode({ controls: ["next"] })],
      "POST /api/sessions/tv/command": () => undefined,
    });
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Next on Living room TV" }));
    expect(within(panel).getByRole("status")).toHaveTextContent("Waiting for Living room TV");
    await waitFor(() => expect(within(panel).getByRole("status")).toBeEmptyDOMElement());
    expect(within(panel).queryByRole("alert")).toBeNull();
  });

  it("numbers devices of the same name", async () => {
    mockApi({ "GET /api/sessions": () => [episode(), episode({ session_id: "tv2" }), live] });
    renderWithProviders(<RemoteButton />);
    const { panel } = await openRemote();
    expect(within(panel).getAllByRole("group").map((g) => g.getAttribute("aria-label"))).toEqual([
      "Living room TV (1)",
      "Living room TV (2)",
      "Unknown device",
    ]);
    expect(within(panel).getByRole("button", { name: "Stop on Living room TV (2)" })).toBeInTheDocument();
  });

  it("gives focus back to the button on Escape, and opens closed when something plays again", async () => {
    let sessions = [episode()];
    mockApi({ "GET /api/sessions": () => sessions });
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
    mockApi({
      "GET /api/sessions": () => [episode({ controls: ["next"] })],
      "POST /api/sessions/tv/command": () => apiError(429, "too_many_requests", "slow down"),
    });
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Next on Living room TV" }));
    expect(await within(panel).findByRole("alert")).toHaveTextContent("Too many at once: try again in a moment.");
  });

  it("stops a device, and goes once nothing else plays", async () => {
    let sessions = [episode()];
    const requests = mockApi({
      "GET /api/sessions": () => sessions,
      "DELETE /api/sessions/tv": () => {
        sessions = [];
        return undefined;
      },
    });
    renderWithProviders(<RemoteButton />);
    const { user, panel } = await openRemote();

    await user.click(within(panel).getByRole("button", { name: "Stop on Living room TV" }));
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/sessions/tv")).toBe(true);
    await waitFor(() => expect(screen.queryByRole("button", { name: /^Remote/ })).toBeNull());
  });
});
