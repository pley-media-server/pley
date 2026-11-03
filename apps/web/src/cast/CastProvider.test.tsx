import type { QueryClient } from "@tanstack/react-query";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { castKeys, patchCastState } from "@/api/cast";
import type { CastDevice, CastState, ItemSummary } from "@/api/types";
import { playUrl } from "@/lib/usePlayItem";
import { makeItem } from "@/test/fixtures";
import { LocationDisplay, mockApi, renderWithProviders, testQueryClient, type FakeRequest } from "@/test/render";
import { CastBar } from "./CastBar";
import { CastButton } from "./CastButton";
import { CastProvider } from "./CastProvider";
import { ANSWER_WITHIN_MS } from "./CastRemote";
import type { CastRequest } from "./context";

const movie = makeItem({ id: 7, title: "A Movie", year: 2001, duration_ms: 6_000_000 });
const other = makeItem({ id: 8, title: "Another Movie", duration_ms: 3_000_000 });

const tv = (state: CastState | null = null): CastDevice => ({ id: "tv-1", name: "Living room TV", platform: "android_tv", state });
const playing = (item: ItemSummary, overrides: Partial<CastState> = {}): CastState => ({
  item,
  position_ms: 60_000,
  duration_ms: item.duration_ms ?? null,
  state: "playing",
  age_ms: 0,
  ...overrides,
});

/** The server: the profile's devices are `devices()`; playing and steering answer 204. */
function serve(devices: () => CastDevice[]): FakeRequest[] {
  return mockApi({
    "GET /api/cast/devices": devices,
    "POST /api/cast/devices/tv-1/play": () => undefined,
    "POST /api/cast/devices/tv-1/control": () => undefined,
  });
}

function setup(request: Partial<CastRequest> = {}) {
  const client = testQueryClient();
  renderWithProviders(
    <CastProvider>
      <CastButton request={() => ({ item: movie, ...request })} />
      <CastBar />
      <LocationDisplay />
    </CastProvider>,
    { client, route: "/item/x" },
  );
  return client;
}

const sent = (requests: FakeRequest[], what: "play" | "control") =>
  requests.filter((r) => r.method === "POST" && r.path === `/api/cast/devices/tv-1/${what}`).map((r) => r.body);

/** The device says what it plays, as the event stream tells it; the pages show it once the list's observers hear. */
async function tell(client: QueryClient, state: CastState | null) {
  await act(async () => {
    patchCastState(client, "tv-1", state);
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

const castButton = () => screen.findByRole("button", { name: "Play on another device" });

async function pickTv(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await castButton());
  const picker = await screen.findByRole("dialog", { name: "Play on" });
  await user.click(within(picker).getByRole("button", { name: /Living room TV/ }));
}

afterEach(() => {
  vi.useRealTimers();
});

describe("casting from here", () => {
  it("is offered only while the profile has another device to cast to", async () => {
    let devices: CastDevice[] = [];
    serve(() => devices);
    const client = setup();
    await waitFor(() => expect(client.getQueryState(castKeys.devices)?.status).toBe("success"));
    expect(screen.queryByRole("button", { name: "Play on another device" })).not.toBeInTheDocument();

    // A device connects: the event stream asks for the list again.
    devices = [tv()];
    await act(() => client.invalidateQueries({ queryKey: castKeys.devices }));
    expect(await castButton()).toBeInTheDocument();
  });

  it("isn't offered for this device itself, in the desktop app", async () => {
    (window as unknown as { __PLEY_APP__?: string }).__PLEY_APP__ = "desktop";
    window.localStorage.setItem("pley.castDevice", "0123456789abcdef0123456789abcdef");
    try {
      serve(() => [{ id: "0123456789abcdef0123456789abcdef", name: "Study PC", platform: "desktop", state: null }]);
      const client = setup();
      await waitFor(() => expect(client.getQueryState(castKeys.devices)?.status).toBe("success"));
      expect(screen.queryByRole("button", { name: "Play on another device" })).not.toBeInTheDocument();
    } finally {
      delete (window as unknown as { __PLEY_APP__?: string }).__PLEY_APP__;
    }
  });

  it("lists each device with what it plays", async () => {
    const user = userEvent.setup();
    serve(() => [tv(playing(other)), { id: "phone", name: "Phone", platform: "android", state: null }]);
    setup();
    await user.click(await castButton());
    const picker = await screen.findByRole("dialog", { name: "Play on" });
    expect(within(picker).getByRole("button", { name: /Living room TV/ })).toHaveTextContent("Playing Another Movie");
    expect(within(picker).getByRole("button", { name: /Phone/ })).not.toHaveTextContent("Playing");
  });

  it("sends a title page's title without a position: the device starts where the profile left off", async () => {
    const user = userEvent.setup();
    const requests = serve(() => [tv()]);
    setup({ fileId: 3 });
    await pickTv(user);
    await waitFor(() => expect(sent(requests, "play")).toEqual([{ item_id: 7, file_id: 3 }]));
    expect(await screen.findByText("Waiting for Living room TV…")).toBeInTheDocument();
  });

  it("sends where a player is, and the player steps aside", async () => {
    const user = userEvent.setup();
    const requests = serve(() => [tv()]);
    const onSent = vi.fn();
    setup({ fileId: 3, position: () => 754_321.4, onSent });
    await pickTv(user);
    await waitFor(() => expect(onSent).toHaveBeenCalledTimes(1));
    expect(sent(requests, "play")).toEqual([{ item_id: 7, file_id: 3, position_ms: 754_321 }]);
  });

  it("opens the remote of a device that plays something, rather than sending", async () => {
    const user = userEvent.setup();
    const requests = serve(() => [tv(playing(other))]);
    const onSent = vi.fn();
    setup({ onSent });
    await pickTv(user);
    const remote = await screen.findByRole("dialog", { name: "Living room TV" });
    expect(remote).toHaveTextContent("Another Movie");
    expect(within(remote).getByText("Playing on Living room TV")).toBeInTheDocument();
    expect(sent(requests, "play")).toEqual([]);
    expect(onSent).not.toHaveBeenCalled();
  });

  it("waits for the device to say it plays the title, and moves its position on", async () => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    const user = userEvent.setup();
    serve(() => [tv()]);
    const client = setup();
    await pickTv(user);
    const remote = await screen.findByRole("dialog", { name: "Living room TV" });
    expect(within(remote).getByText("Waiting for Living room TV…")).toBeInTheDocument();

    // Another title isn't the answer; the one sent, loading, is.
    await tell(client, playing(other));
    expect(within(remote).getByText("Waiting for Living room TV…")).toBeInTheDocument();
    await tell(client, playing(movie, { state: "buffering", position_ms: 0 }));
    expect(within(remote).queryByText("Waiting for Living room TV…")).not.toBeInTheDocument();
    expect(within(remote).getByText("Loading on Living room TV")).toBeInTheDocument();

    await tell(client, playing(movie, { position_ms: 60_000, age_ms: 2_000 }));
    expect(within(remote).getByText("1:02")).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(5_000);
    });
    expect(within(remote).getByText("1:07")).toBeInTheDocument();

    // Paused, it stands still.
    await tell(client, playing(movie, { state: "paused", position_ms: 70_000 }));
    act(() => {
      vi.advanceTimersByTime(5_000);
    });
    expect(within(remote).getByText("1:10")).toBeInTheDocument();
    expect(within(remote).getByRole("button", { name: "Play" })).toBeInTheDocument();
  });

  it("says so when the device doesn't answer within 15 s", async () => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    const user = userEvent.setup();
    serve(() => [tv()]);
    setup();
    await pickTv(user);
    const remote = await screen.findByRole("dialog", { name: "Living room TV" });
    act(() => {
      vi.advanceTimersByTime(ANSWER_WITHIN_MS - 1_000);
    });
    expect(within(remote).getByText("Waiting for Living room TV…")).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    expect(within(remote).getByText("Living room TV didn't answer")).toBeInTheDocument();
    await user.click(within(remote).getAllByRole("button", { name: "Close" })[0]!);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("steers the device: play and pause, seeks back 10 s and on 30 s, stop", async () => {
    const user = userEvent.setup();
    const requests = serve(() => [tv(playing(other, { state: "paused", position_ms: 600_000 }))]);
    setup();
    await pickTv(user);
    const remote = await screen.findByRole("dialog", { name: "Living room TV" });
    await user.click(within(remote).getByRole("button", { name: "Play" }));
    await waitFor(() => expect(within(remote).getByRole("button", { name: "Pause" })).toBeInTheDocument());
    await user.click(within(remote).getByRole("button", { name: "Pause" }));
    await waitFor(() => expect(sent(requests, "control")).toHaveLength(2));
    await user.click(within(remote).getByRole("button", { name: "Back 10 seconds" }));
    await waitFor(() => expect(sent(requests, "control")).toHaveLength(3));
    await user.click(within(remote).getByRole("button", { name: "Forward 30 seconds" }));
    await waitFor(() => expect(sent(requests, "control")).toHaveLength(4));
    await user.click(within(remote).getByRole("button", { name: "Stop" }));
    await waitFor(() => expect(sent(requests, "control")).toHaveLength(5));
    // Paused at 10:00 when asked to play, and when asked to pause right after (the clock stands still in a test).
    const [resume, pause, back, forward, stop] = sent(requests, "control") as { action: string; position_ms?: number }[];
    expect(resume).toEqual({ action: "resume" });
    expect(pause).toEqual({ action: "pause" });
    expect(back?.action).toBe("seek");
    expect(back?.position_ms).toBeGreaterThanOrEqual(590_000);
    expect(back?.position_ms).toBeLessThan(591_000);
    expect(forward?.action).toBe("seek");
    expect(forward?.position_ms).toBeGreaterThanOrEqual(620_000);
    expect(forward?.position_ms).toBeLessThan(621_000);
    expect(stop).toEqual({ action: "stop" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("plays here, from where the device was, and stops it", async () => {
    const user = userEvent.setup();
    const requests = serve(() => [tv(playing(other, { state: "paused", position_ms: 600_000 }))]);
    setup();
    await pickTv(user);
    const remote = await screen.findByRole("dialog", { name: "Living room TV" });
    await user.click(within(remote).getByRole("button", { name: "Play here" }));
    await waitFor(() => expect(sent(requests, "control")).toEqual([{ action: "stop" }]));
    expect(screen.getByTestId("location")).toHaveTextContent(playUrl(8, { startMs: 600_000 }));
  });

  it("shows what the device last cast to plays at the bottom, until it plays nothing", async () => {
    const user = userEvent.setup();
    serve(() => [tv()]);
    const client = setup();
    await pickTv(user);
    await screen.findByRole("dialog", { name: "Living room TV" });
    expect(screen.queryByText("Playing on Living room TV")).not.toBeInTheDocument();
    await tell(client, playing(movie));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Close" }));

    const bar = screen.getByText("Playing on Living room TV");
    expect(screen.getByText("A Movie (2001)")).toBeInTheDocument();
    await user.click(bar);
    expect(await screen.findByRole("dialog", { name: "Living room TV" })).toBeInTheDocument();

    await tell(client, null);
    expect(screen.queryByText("Playing on Living room TV")).not.toBeInTheDocument();
    expect(screen.getByText("Nothing is playing on Living room TV.")).toBeInTheDocument();
  });
});
