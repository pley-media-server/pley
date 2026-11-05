import { act, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ServerEvent } from "@/api/types";
import { receiverId, receiverQuery } from "@/lib/castReceiver";
import { playUrl } from "@/lib/usePlayItem";
import { makeItem } from "@/test/fixtures";
import { LocationDisplay, mockApi, renderWithProviders } from "@/test/render";
import { CastReceiver } from "./CastReceiver";

// The stream's events, sent by the tests; what the receiver asks of it, recorded.
const events = vi.hoisted(() => ({ listeners: new Set<(ev: ServerEvent) => void>(), receiver: vi.fn<(q: string | null) => void>() }));
vi.mock("@/api/events", async (original) => ({
  ...(await original<typeof import("@/api/events")>()),
  onServerEvent: (listener: (ev: ServerEvent) => void) => {
    events.listeners.add(listener);
    return () => events.listeners.delete(listener);
  },
  setReceiver: events.receiver,
}));

const send = (ev: ServerEvent) => act(() => events.listeners.forEach((l) => l(ev)));
const desktop = (on: boolean) => {
  (window as { __PLEY_APP__?: string }).__PLEY_APP__ = on ? "desktop" : undefined;
};

function cast(overrides: Partial<Extract<ServerEvent, { type: "cast" }>>): ServerEvent {
  return {
    type: "cast",
    receiver_id: receiverId(),
    item_id: null,
    playlist_id: null,
    channel_id: null,
    start_ms: null,
    shuffle: false,
    audio_index: null,
    subtitle_index: null,
    from: "Phone",
    ...overrides,
  };
}

/** A film the user is a minute and a half into, and a channel. */
function serve(live = false) {
  const film = { ...makeItem({ id: 5, kind: "movie", title: "Test Film" }), files: [], user_state: { position_ms: 90_000, watched: false, play_count: 0 } };
  return mockApi({
    "GET /api/live": () => ({ available: live, allowed: live, channels: live ? 1 : 0, sources: 0, recent: [] }),
    "GET /api/items/5": () => film,
    "GET /api/live/channels/3": () => ({ id: 3, source_id: 1, source_uid: "abc", name: "One", number: "1", group: "", logo: null, favorite: false, has_guide: false, now: null, next: null }),
  });
}

function show(route = "/") {
  return renderWithProviders(
    <>
      <CastReceiver />
      <LocationDisplay />
    </>,
    { route },
  );
}

beforeEach(() => {
  events.listeners.clear();
  events.receiver.mockClear();
});
afterEach(() => {
  desktop(false);
  vi.unstubAllGlobals();
});

describe("CastReceiver", () => {
  it("in a browser, takes no casts", async () => {
    serve();
    show();
    send(cast({ item_id: 5 }));
    await act(() => new Promise((r) => setTimeout(r, 20)));
    expect(screen.getByTestId("location")).toHaveTextContent(/^\/$/);
    expect(events.receiver).not.toHaveBeenCalled();
  });

  it("in the desktop app, is a receiver, of channels while the user has Live TV", async () => {
    desktop(true);
    serve(true);
    const { unmount } = show();
    await waitFor(() => expect(events.receiver).toHaveBeenLastCalledWith(receiverQuery(true)));
    unmount();
    expect(events.receiver).toHaveBeenLastCalledWith(null);
  });

  it("plays a film where the remote said, with the tracks picked there, and says where it came from", async () => {
    desktop(true);
    serve();
    show();
    send(cast({ item_id: 5, start_ms: 60_000, audio_index: 2, subtitle_index: -1 }));
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(playUrl(5, { startMs: 60_000, audio: 2, sub: -1 })));
    expect(await screen.findByText("Playing from Phone")).toBeInTheDocument();
  });

  it("plays a film from where its own Play would, without a place", async () => {
    desktop(true);
    serve();
    show();
    send(cast({ item_id: 5, from: null }));
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(playUrl(5, { startMs: 90_000 })));
    expect(await screen.findByText("Playing from another device")).toBeInTheDocument();
  });

  it("tunes a channel", async () => {
    desktop(true);
    serve(true);
    show();
    send(cast({ channel_id: 3 }));
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent("/live/abc/3"));
  });

  it("leaves another device's casts, and any while a profile is chosen", async () => {
    desktop(true);
    serve();
    show("/profiles");
    send(cast({ item_id: 5 }));
    send(cast({ item_id: 5, receiver_id: "tv-1" }));
    await act(() => new Promise((r) => setTimeout(r, 20)));
    expect(screen.getByTestId("location")).toHaveTextContent("/profiles");
  });
});
