import { fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { resetPrefetchPlay } from "@/lib/prefetchPlay";
import { makeItem } from "@/test/fixtures";
import { mockApi, renderWithProviders } from "@/test/render";
import { MediaCard } from "./cards";

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("MediaCard", () => {
  beforeEach(() => resetPrefetchPlay());
  afterEach(() => vi.unstubAllGlobals());

  it("reads the video ahead when its Play button is pointed at or focused", async () => {
    const requests = mockApi({ "POST /api/items/42/prefetch": () => new Response(null, { status: 202 }) });
    const item = makeItem({ id: 42, kind: "episode", title: "Pilot", user_state: { position_ms: 600_000, watched: false, play_count: 1 } });
    renderWithProviders(<MediaCard item={item} width={200} continueStyle />);
    const play = screen.getByRole("button", { name: /^Play / });
    fireEvent.pointerEnter(play);
    fireEvent.focus(play);
    await settle();
    // Once: the server reads a file ahead at most every two minutes anyway.
    expect(requests.map((r) => `${r.method} ${r.path}`)).toEqual(["POST /api/items/42/prefetch"]);
  });

  it("draws quality badges only when asked to", () => {
    const media = { bitrate: 42_000_000, size: 1024, width: 3840, height: 1600, video_codec: "hevc", dynamic_range: "dolby_vision" as const };
    const item = makeItem({ id: 44, title: "Brazil", media });
    const { unmount } = renderWithProviders(<MediaCard item={item} width={200} />);
    expect(screen.queryByText("4K")).not.toBeInTheDocument();
    unmount();
    renderWithProviders(<MediaCard item={item} width={200} badges />);
    expect(screen.getByText("4K")).toBeInTheDocument();
    expect(screen.getByText("DV")).toBeInTheDocument();
  });

  it("leaves music alone", async () => {
    const requests = mockApi({});
    renderWithProviders(<MediaCard item={makeItem({ id: 43, kind: "album" })} width={200} />);
    fireEvent.pointerEnter(screen.getByRole("button", { name: /^Play / }));
    await settle();
    expect(requests).toEqual([]);
  });
});
