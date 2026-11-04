import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { isPlayingHere, playingHere, useSessionsHere } from "./playingHere";

describe("playingHere", () => {
  it("holds a session until its player lets go, and says so to whoever shows it", () => {
    const { result } = renderHook(() => useSessionsHere());
    expect(result.current.has("a")).toBe(false);

    let leave = () => undefined as void;
    act(() => {
      leave = playingHere("a");
    });
    const leaveB = playingHere("b");
    expect(isPlayingHere("a")).toBe(true);
    expect(result.current.has("a")).toBe(true);

    act(() => leave());
    expect(isPlayingHere("a")).toBe(false);
    expect(result.current.has("a")).toBe(false);
    expect(result.current.has("b")).toBe(true);
    // Twice changes nothing.
    act(() => leave());
    expect(isPlayingHere("b")).toBe(true);
    act(() => leaveB());
    expect(result.current.size).toBe(0);
  });
});
