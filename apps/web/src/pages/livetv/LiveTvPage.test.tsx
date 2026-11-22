import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { mockApi, renderWithProviders } from "@/test/render";
import { LiveTvPage } from "./LiveTvPage";

function live() {
  mockApi({
    "GET /api/live": () => ({ allowed: true, available: true }),
    "GET /api/live/groups": () => ({ favorites: 0, recent: 0, total: 0, groups: [] }),
    "GET /api/live/browse": () => ({ channels: [], total: 0, offset: 0 }),
    "GET /api/live/guide": () => ({ channels: [], programs: [] }),
  });
}

describe("LiveTvPage", () => {
  it("puts the categories away for a wider guide, and remembers it", async () => {
    const user = userEvent.setup();
    localStorage.clear();
    live();
    const first = renderWithProviders(<LiveTvPage />);
    const hide = await screen.findByRole("button", { name: "Hide categories" });
    expect(hide).toHaveAttribute("aria-expanded", "true");
    const rail = document.getElementById("live-categories");
    expect(rail).toHaveClass("lg:block");
    // The category is picked from the button above the guide, on every screen.
    const picker = screen.getAllByRole("button").find((b) => !b.closest("nav") && b.textContent?.startsWith("All channels"));
    expect(picker).toHaveClass("lg:hidden");

    await user.click(hide);
    const show = screen.getByRole("button", { name: "Show categories" });
    expect(show).toHaveAttribute("aria-expanded", "false");
    expect(rail).not.toHaveClass("lg:block");
    expect(picker).not.toHaveClass("lg:hidden");
    await waitFor(() => expect(localStorage.getItem("pley.live.railHidden")).toBe("true"));

    // Opened again, it is still put away.
    first.unmount();
    live();
    renderWithProviders(<LiveTvPage />);
    expect(await screen.findByRole("button", { name: "Show categories" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Show categories" }));
    expect(document.getElementById("live-categories")).toHaveClass("lg:block");
  });
});
