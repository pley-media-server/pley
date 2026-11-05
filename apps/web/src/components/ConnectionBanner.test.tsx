import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { setConnectionLost } from "@/lib/connection";
import { SWITCH_SERVER_PATH } from "@/lib/shell";
import { ConnectionBanner } from "./ConnectionBanner";

afterEach(() => {
  setConnectionLost(false);
  delete (window as { __PLEY_APP__?: unknown }).__PLEY_APP__;
});

describe("ConnectionBanner", () => {
  it("shows while the server is out of reach", () => {
    render(<ConnectionBanner />);
    expect(screen.queryByRole("status")).toBeNull();

    act(() => setConnectionLost(true));
    expect(screen.getByRole("status")).toHaveTextContent("Pley is offline");
    expect(screen.queryByRole("link", { name: "Downloads" })).toBeNull();

    act(() => setConnectionLost(false));
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("leads to the downloads in the desktop app", () => {
    (window as { __PLEY_APP__?: unknown }).__PLEY_APP__ = "desktop";
    setConnectionLost(true);
    render(<ConnectionBanner />);
    expect(screen.getByRole("link", { name: "Downloads" })).toHaveAttribute("href", SWITCH_SERVER_PATH);
  });
});
