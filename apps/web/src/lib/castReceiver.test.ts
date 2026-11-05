import { afterEach, describe, expect, it } from "vitest";
import { deviceFields, receiverId, receiverName, receiverQuery, takesCasts } from "./castReceiver";

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

  it("asks for channels only while the user has Live TV", () => {
    const query = (live: boolean) => Object.fromEntries(new URLSearchParams(receiverQuery(live)));
    expect(query(false)).toEqual({ receiver: receiverId(), device_name: receiverName(), device_kind: "computer", casts: "item" });
    expect(query(true).casts).toBe("item,live");
  });
});
