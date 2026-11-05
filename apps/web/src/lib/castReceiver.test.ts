import { afterEach, describe, expect, it, vi } from "vitest";
import {
  castOf,
  castState,
  deviceFields,
  dropHandedOver,
  handOver,
  receiverId,
  receiverName,
  receiverQuery,
  switchesProfile,
  takeHandedOver,
  takesCasts,
  type Cast,
} from "./castReceiver";

const desktop = (on: boolean) => {
  (window as { __PLEY_APP__?: string }).__PLEY_APP__ = on ? "desktop" : undefined;
};

afterEach(() => {
  desktop(false);
  localStorage.clear();
});

describe("the desktop app as a receiver of casts", () => {
  it("takes casts in the desktop app only", () => {
    expect(takesCasts()).toBe(false);
    expect(deviceFields()).toEqual({});
    desktop(true);
    expect(takesCasts()).toBe(true);
    expect(deviceFields()).toEqual({ device_name: receiverName(), device_kind: "computer" });
    expect(receiverName()).toMatch(/^Pley for /);
  });

  it("keeps one id, of the characters the server takes", () => {
    const id = receiverId();
    expect(id).toMatch(/^desktop-[A-Za-z0-9._:-]+$/);
    expect(id.length).toBeLessThanOrEqual(128);
    expect(receiverId()).toBe(id);
    // One the server wouldn't take is made anew.
    localStorage.setItem("pley.receiver", "not valid!");
    expect(receiverId()).not.toBe("not valid!");
  });

  it("is named for its system, as the user's remotes list it", () => {
    expect(receiverName("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/140.0")).toBe("Pley for Windows");
    expect(receiverName("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15")).toBe("Pley for Mac");
    expect(receiverName("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15")).toBe("Pley for Linux");
  });

  it("marks the pages a cast opens, and only those", () => {
    expect(castOf(castState())).toEqual(expect.any(Number));
    expect(castOf(null)).toBeNull();
    expect(castOf({ sleep: 30 })).toBeNull();
  });

  it("asks for channels only while the user has Live TV", () => {
    const query = (live: boolean) => Object.fromEntries(new URLSearchParams(receiverQuery(live)));
    expect(query(false)).toEqual({ receiver: receiverId(), device_name: receiverName(), device_kind: "computer", casts: "item" });
    expect(query(true).casts).toBe("item,live");
  });
});

describe("a cast from another profile of the household", () => {
  const cast = (more: Partial<Cast> = {}): Cast => ({
    type: "cast",
    receiver_id: "desktop-1",
    item_id: 5,
    playlist_id: null,
    channel_id: null,
    start_ms: null,
    shuffle: false,
    from: "Phone",
    profile_id: 2,
    switch_ticket: "t1",
    ...more,
  });

  afterEach(() => {
    vi.useRealTimers();
    takeHandedOver(2);
  });

  it("switches the device to the caster's profile, and one from the profile it is on doesn't", () => {
    expect(switchesProfile(cast(), 1)).toBe(true);
    expect(switchesProfile(cast(), 2)).toBe(false);
    expect(switchesProfile(cast({ profile_id: undefined, switch_ticket: undefined }), 1)).toBe(false);
  });

  it("is handed over to the receiver of the profile it switched to, once", () => {
    const sent = cast();
    handOver(sent);
    expect(takeHandedOver(1)).toBeNull();
    expect(takeHandedOver(2)).toBe(sent);
    expect(takeHandedOver(2)).toBeNull();
  });

  it("is dropped when its switch didn't go through, and the one after it kept", () => {
    const first = cast();
    const second = cast({ item_id: 6 });
    handOver(first);
    handOver(second);
    dropHandedOver(first);
    expect(takeHandedOver(2)).toBe(second);
    handOver(first);
    dropHandedOver(first);
    expect(takeHandedOver(2)).toBeNull();
  });

  it("isn't played once its ticket would have expired", () => {
    vi.useFakeTimers();
    handOver(cast());
    vi.advanceTimersByTime(61_000);
    expect(takeHandedOver(2)).toBeNull();
  });
});
