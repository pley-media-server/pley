import { useRef } from "react";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ItemDetail, ServerEvent } from "@/api/types";
import { playUrl } from "@/lib/usePlayItem";
import { makeItem } from "@/test/fixtures";
import { LocationDisplay, mockApi, renderWithProviders, type FakeRequest } from "@/test/render";
import type { CastSelf } from "./device";
import { castReportsSent, REPORT_INTERVAL_MS, useCastCommands, useCastPlayer, type CastPlayer } from "./receiver";

const listeners = vi.hoisted(() => new Set<(ev: ServerEvent) => void>());
vi.mock("@/api/events", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  onServerEvent: (listener: (ev: ServerEvent) => void) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
}));

const emit = (ev: ServerEvent) => act(() => listeners.forEach((l) => l(ev)));

const self: CastSelf = { id: "0123456789abcdef0123456789abcdef", name: "Study PC", platform: "desktop" };
const statePath = `/api/cast/devices/${self.id}/state`;

function Commands() {
  useCastCommands(self);
  return <LocationDisplay />;
}

function detail(userState: ItemDetail["user_state"]): ItemDetail {
  return {
    ...makeItem({ id: 7, title: "A Movie", user_state: userState }),
    original_title: null,
    tagline: null,
    genres: [],
    studio: null,
    content_rating: null,
    tmdb_id: null,
    imdb_id: null,
    files: [],
  };
}

describe("useCastCommands", () => {
  it("opens the player for what it is sent, where it was sent from", async () => {
    renderWithProviders(<Commands />, { route: "/item/x" });
    emit({ type: "cast_play", device_id: self.id, item_id: 7, file_id: 3, position_ms: 754_000 });
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(playUrl(7, { startMs: 754_000, fileId: 3 })));
  });

  it("starts where the profile left off, without asking, when no position is sent", async () => {
    mockApi({ "GET /api/items/7": () => detail({ position_ms: 120_000, watched: false, play_count: 0 }) });
    renderWithProviders(<Commands />);
    emit({ type: "cast_play", device_id: self.id, item_id: 7, file_id: null, position_ms: null });
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(playUrl(7, { startMs: 120_000 })));
  });

  it("starts a title watched before from the start", async () => {
    mockApi({ "GET /api/items/7": () => detail({ position_ms: 120_000, watched: true, play_count: 1 }) });
    renderWithProviders(<Commands />);
    emit({ type: "cast_play", device_id: self.id, item_id: 7, file_id: null, position_ms: null });
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(playUrl(7)));
  });

  it("leaves another device's commands alone", async () => {
    renderWithProviders(<Commands />, { route: "/item/x" });
    emit({ type: "cast_play", device_id: "another-device", item_id: 7, file_id: null, position_ms: 0 });
    await act(() => Promise.resolve());
    expect(screen.getByTestId("location")).toHaveTextContent("/item/x");
  });
});

const controls = () => ({ pause: vi.fn(), resume: vi.fn(), seek: vi.fn(), stop: vi.fn() }) satisfies CastPlayer;

function Player({ controls, itemId = 7 }: { controls: CastPlayer; itemId?: number }) {
  const video = useRef<HTMLVideoElement>(null);
  useCastPlayer(self, { itemId, video, position: () => ({ position_ms: 60_000.4, duration_ms: 6_000_000 }), controls });
  return <video ref={video} data-testid="video" />;
}

/** The reports the server heard, in order. */
const reports = (requests: FakeRequest[]) => requests.filter((r) => r.method === "PUT" && r.path === statePath).map((r) => r.body);

const at = (state: string) => ({ item_id: 7, position_ms: 60_000, duration_ms: 6_000_000, state });

describe("useCastPlayer", () => {
  afterEach(async () => {
    vi.useRealTimers();
    // A test's last reports don't reach the next one's server.
    await castReportsSent();
  });

  it("is steered by cast commands while it is open, and only then", () => {
    mockApi({});
    const steered = controls();
    const { unmount } = renderWithProviders(
      <>
        <Commands />
        <Player controls={steered} />
      </>,
    );
    emit({ type: "cast_control", device_id: self.id, action: "pause", position_ms: null });
    emit({ type: "cast_control", device_id: self.id, action: "resume", position_ms: null });
    emit({ type: "cast_control", device_id: self.id, action: "seek", position_ms: 90_000 });
    emit({ type: "cast_control", device_id: "another-device", action: "stop", position_ms: null });
    emit({ type: "cast_control", device_id: self.id, action: "stop", position_ms: null });
    expect(steered.pause).toHaveBeenCalledTimes(1);
    expect(steered.resume).toHaveBeenCalledTimes(1);
    expect(steered.seek).toHaveBeenCalledWith(90_000);
    expect(steered.stop).toHaveBeenCalledTimes(1);

    unmount();
    renderWithProviders(<Commands />);
    emit({ type: "cast_control", device_id: self.id, action: "pause", position_ms: null });
    expect(steered.pause).toHaveBeenCalledTimes(1);
  });

  it("says what it plays as it starts, pauses, seeks and ends", async () => {
    const requests = mockApi({ [`PUT ${statePath}`]: () => undefined });
    const { unmount } = renderWithProviders(<Player controls={controls()} />);
    const video = screen.getByTestId("video");
    fireEvent(video, new Event("playing"));
    fireEvent(video, new Event("waiting"));
    fireEvent(video, new Event("pause"));
    fireEvent(video, new Event("seeked"));
    fireEvent(video, new Event("ended"));
    unmount();
    await waitFor(() =>
      expect(reports(requests)).toEqual([at("buffering"), at("playing"), at("buffering"), at("paused"), at("paused"), { item_id: null }]),
    );
  });

  it("says it plays nothing once it closes, and where it is every 10 s while it plays", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const requests = mockApi({ [`PUT ${statePath}`]: () => undefined });
    const { unmount } = renderWithProviders(<Player controls={controls()} />);
    const video = screen.getByTestId("video");
    // Paused: nothing more to say.
    act(() => {
      vi.advanceTimersByTime(REPORT_INTERVAL_MS);
    });
    Object.defineProperty(video, "paused", { value: false, configurable: true });
    act(() => {
      vi.advanceTimersByTime(REPORT_INTERVAL_MS);
    });
    unmount();
    vi.useRealTimers();
    await waitFor(() => expect(reports(requests)).toEqual([at("buffering"), at("playing"), { item_id: null }]));
  });

  it("goes on after a report fails", async () => {
    const requests = mockApi({});
    const { unmount } = renderWithProviders(<Player controls={controls()} />);
    unmount();
    await waitFor(() => expect(requests.filter((r) => r.method === "PUT")).toHaveLength(2));
  });
});
