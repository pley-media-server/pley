import { render, renderHook, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ItemDetail, Marker, MediaFile } from "@/api/types";
import { makeItem, makeMarker } from "@/test/fixtures";
import { activeMarker, fileMarkers, SkipButton, skipTarget, useSkipMarkers } from "./SkipButton";

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

describe("useSkipMarkers", () => {
  type Props = { seconds: number; enabled?: boolean; markers?: Marker[] };

  function setup(initial: Props) {
    const onCredits = vi.fn();
    const hook = renderHook(
      ({ seconds, enabled = true, markers = [intro, credits] }: Props) =>
        useSkipMarkers({ markers, seconds, enabled, onCredits }),
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

  it("does nothing while disabled", () => {
    const { result, onCredits } = setup({ seconds: 1_201, enabled: false });
    expect(result.current).toBeNull();
    expect(onCredits).not.toHaveBeenCalled();
  });
});

describe("SkipButton", () => {
  it("names the segment and skips it", async () => {
    const onSkip = vi.fn();
    render(<SkipButton marker={credits} onSkip={onSkip} lifted={false} />);
    await userEvent.click(screen.getByRole("button", { name: /Skip Credits/ }));
    expect(onSkip).toHaveBeenCalledTimes(1);
  });
});
