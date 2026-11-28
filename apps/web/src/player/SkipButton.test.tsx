import { act, render, renderHook, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ItemDetail, Marker, MediaFile } from "@/api/types";
import { makeItem, makeMarker } from "@/test/fixtures";
import { activeMarker, fileMarkers, reachesEnd, SKIP_ALONE_MS, SkipButton, skipTarget, useSkipAlone, useSkipMarkers } from "./SkipButton";

const intro = makeMarker({ kind: "intro", start_ms: 10_000, end_ms: 70_000 });
const credits = makeMarker({ kind: "credits", start_ms: 1_200_000, end_ms: 1_300_000 });

describe("fileMarkers", () => {
  const item = {
    ...makeItem(),
    files: [{ id: 1 }, { id: 2 }] as MediaFile[],
    markers: [intro, makeMarker({ file_id: 2, start_ms: 0, end_ms: 5_000 })],
  } as ItemDetail;

  it("takes the markers of the file being played, else the first file", () => {
    expect(fileMarkers(item, 2)).toEqual([item.markers?.[1]]);
    expect(fileMarkers(item, null)).toEqual([intro]);
    expect(fileMarkers({ ...item, markers: undefined }, 1)).toEqual([]);
  });
});

describe("activeMarker", () => {
  it("finds the segment the playhead is in, until a second before it ends", () => {
    expect(activeMarker([intro, credits], 9_900)).toBeNull();
    expect(activeMarker([intro, credits], 10_000)).toBe(intro);
    expect(activeMarker([intro, credits], 68_999)).toBe(intro);
    expect(activeMarker([intro, credits], 69_000)).toBeNull();
    expect(activeMarker([intro, credits], 1_250_000)).toBe(credits);
  });

  it("leaves out a kind it has no button for", () => {
    const recap = makeMarker({ kind: "recap" as Marker["kind"], start_ms: 0, end_ms: 60_000 });
    expect(activeMarker([recap, intro], 5_000)).toBeNull();
    expect(activeMarker([recap, intro], 20_000)).toBe(intro);
  });
});

describe("skipTarget", () => {
  it("lands just before an intro ends, so its last moment plays, and at the end of credits", () => {
    expect(skipTarget(intro)).toBe(69_250);
    expect(skipTarget(credits)).toBe(1_300_000);
  });

  it("stays inside a very short intro", () => {
    expect(skipTarget(makeMarker({ kind: "intro", start_ms: 0, end_ms: 1_000 }))).toBe(500);
  });
});

describe("reachesEnd", () => {
  it("takes credits ending within 15 s of the end of their file as running to it", () => {
    expect(reachesEnd(credits, 1_300_000)).toBe(true);
    expect(reachesEnd(credits, 1_315_000)).toBe(true);
    expect(reachesEnd(credits, 1_315_001)).toBe(false);
  });

  it("leaves out intros and files of unknown length", () => {
    expect(reachesEnd(makeMarker({ kind: "intro", start_ms: 0, end_ms: 60_000 }), 60_000)).toBe(false);
    expect(reachesEnd(credits, 0)).toBe(false);
  });
});

describe("useSkipMarkers", () => {
  // The file ends with the credits unless a test says otherwise.
  type Props = { seconds: number; enabled?: boolean; markers?: Marker[]; durationMs?: number };

  function setup(initial: Props) {
    const onCredits = vi.fn();
    const hook = renderHook(
      ({ seconds, enabled = true, markers = [intro, credits], durationMs = credits.end_ms }: Props) =>
        useSkipMarkers({ markers, seconds, durationMs, enabled, onCredits }),
      { initialProps: initial },
    );
    return { ...hook, onCredits };
  }

  it("offers to skip an intro, and nothing more", () => {
    const { result, rerender, onCredits } = setup({ seconds: 5 });
    expect(result.current).toBeNull();

    rerender({ seconds: 12 });
    expect(result.current).toBe(intro);
    expect(onCredits).not.toHaveBeenCalled();
  });

  it("announces the credits once, even when seeking back into them", () => {
    const { result, rerender, onCredits } = setup({ seconds: 1_000 });
    rerender({ seconds: 1_201 });
    expect(result.current).toBe(credits);
    expect(onCredits).toHaveBeenCalledWith(credits);

    rerender({ seconds: 1_000 });
    rerender({ seconds: 1_210 });
    expect(onCredits).toHaveBeenCalledTimes(1);
  });

  it("announces credits ending 10 s before the end of the file", () => {
    const { result, onCredits } = setup({ seconds: 1_201, durationMs: 1_310_000 });
    expect(result.current).toBe(credits);
    expect(onCredits).toHaveBeenCalledWith(credits);
  });

  it("only offers to skip credits that something follows, 60 s before the end", () => {
    const { result, onCredits } = setup({ seconds: 1_201, durationMs: 1_360_000 });
    expect(result.current).toBe(credits);
    expect(onCredits).not.toHaveBeenCalled();
  });

  it("only offers to skip credits in a file of unknown length", () => {
    const { result, onCredits } = setup({ seconds: 1_201, durationMs: 0 });
    expect(result.current).toBe(credits);
    expect(onCredits).not.toHaveBeenCalled();
  });

  it("announces the credits once the file's length becomes known inside them", () => {
    const { rerender, onCredits } = setup({ seconds: 1_201, durationMs: 0 });
    expect(onCredits).not.toHaveBeenCalled();
    rerender({ seconds: 1_203, durationMs: credits.end_ms });
    expect(onCredits).toHaveBeenCalledTimes(1);
    rerender({ seconds: 1_205, durationMs: credits.end_ms });
    expect(onCredits).toHaveBeenCalledTimes(1);
  });

  it("does nothing while disabled", () => {
    const { result, onCredits } = setup({ seconds: 1_201, enabled: false });
    expect(result.current).toBeNull();
    expect(onCredits).not.toHaveBeenCalled();
  });
});

describe("useSkipAlone", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  // Skip can show unless a test says otherwise.
  type Props = { marker: Marker | null; showable?: boolean };
  const hook = (initial: Props = { marker: null }) =>
    renderHook(({ marker, showable = true }: Props) => useSkipAlone(marker, showable), { initialProps: initial });

  it("shows Skip on its own for 8 s from entering a segment, then not", () => {
    const { result, rerender } = hook();
    expect(result.current).toBe(false);
    rerender({ marker: intro });
    expect(result.current).toBe(true);
    act(() => {
      vi.advanceTimersByTime(SKIP_ALONE_MS - 1);
    });
    expect(result.current).toBe(true);
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current).toBe(false);
  });

  it("doesn't start over while the playhead stays in the segment", () => {
    const { result, rerender } = hook();
    rerender({ marker: intro });
    act(() => {
      vi.advanceTimersByTime(5_000);
    });
    rerender({ marker: { ...intro } });
    act(() => {
      vi.advanceTimersByTime(3_000);
    });
    expect(result.current).toBe(false);
  });

  it("starts over on entering the segment again, or another", () => {
    const { result, rerender } = hook();
    rerender({ marker: intro });
    act(() => {
      vi.advanceTimersByTime(SKIP_ALONE_MS);
    });
    rerender({ marker: null });
    expect(result.current).toBe(false);
    rerender({ marker: intro });
    expect(result.current).toBe(true);
    act(() => {
      vi.advanceTimersByTime(SKIP_ALONE_MS);
    });
    rerender({ marker: credits });
    expect(result.current).toBe(true);
  });

  it("counts its 8 s from when Skip can show, however long a start inside the segment takes", () => {
    const { result, rerender } = hook({ marker: intro, showable: false });
    act(() => {
      vi.advanceTimersByTime(SKIP_ALONE_MS + 5_000);
    });
    rerender({ marker: intro, showable: true });
    expect(result.current).toBe(true);
    act(() => {
      vi.advanceTimersByTime(SKIP_ALONE_MS - 1);
    });
    expect(result.current).toBe(true);
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current).toBe(false);
  });

  it("counts on while the stream loads again inside the segment", () => {
    const { result, rerender } = hook({ marker: intro });
    act(() => {
      vi.advanceTimersByTime(5_000);
    });
    // A seek or another quality: Skip can't show until the stream plays again.
    rerender({ marker: intro, showable: false });
    act(() => {
      vi.advanceTimersByTime(2_000);
    });
    rerender({ marker: intro, showable: true });
    expect(result.current).toBe(true);
    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    expect(result.current).toBe(false);
  });

  it("waits again in a segment entered anew before Skip can show", () => {
    const { result, rerender } = hook({ marker: intro });
    act(() => {
      vi.advanceTimersByTime(SKIP_ALONE_MS);
    });
    rerender({ marker: credits, showable: false });
    act(() => {
      vi.advanceTimersByTime(SKIP_ALONE_MS);
    });
    rerender({ marker: credits, showable: true });
    expect(result.current).toBe(true);
    act(() => {
      vi.advanceTimersByTime(SKIP_ALONE_MS);
    });
    expect(result.current).toBe(false);
  });
});

describe("SkipButton", () => {
  it("names the segment and skips it", async () => {
    const onSkip = vi.fn();
    render(<SkipButton marker={credits} onSkip={onSkip} lifted={false} onHover={() => undefined} />);
    await userEvent.click(screen.getByRole("button", { name: /Skip Credits/ }));
    expect(onSkip).toHaveBeenCalledTimes(1);
  });

  it("says when the mouse comes over it and leaves, and that it left when it goes from under the mouse", async () => {
    const onHover = vi.fn();
    const { unmount } = render(<SkipButton marker={credits} onSkip={() => undefined} lifted onHover={onHover} />);
    const button = screen.getByRole("button", { name: /Skip Credits/ });
    await userEvent.hover(button);
    expect(onHover.mock.calls).toEqual([[true]]);
    await userEvent.unhover(button);
    expect(onHover.mock.calls).toEqual([[true], [false]]);

    await userEvent.hover(button);
    unmount();
    expect(onHover.mock.calls).toEqual([[true], [false], [true], [false]]);
  });

  it("says nothing as it goes with the mouse elsewhere", () => {
    const onHover = vi.fn();
    render(<SkipButton marker={credits} onSkip={() => undefined} lifted onHover={onHover} />).unmount();
    expect(onHover).not.toHaveBeenCalled();
  });
});
