import { Route, Routes } from "react-router";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { LocationDisplay, mockApi, renderWithProviders } from "@/test/render";
import { PlexSetupPage } from "./PlexSetupPage";

describe("PlexSetupPage", () => {
  it("goes back to how to start, where Start fresh is too", async () => {
    const user = userEvent.setup();
    mockApi({
      "GET /api/import/plex": () => ({ state: "idle", step: null, done: 0, total: 0, error: null, report: null, first_pass_done: false }),
      "GET /api/import/plex/pin": () => ({ signed_in: false }),
    });
    renderWithProviders(
      <Routes>
        <Route path="/setup/plex" element={<PlexSetupPage />} />
        <Route path="*" element={<LocationDisplay />} />
      </Routes>,
      { route: "/setup/plex" },
    );
    expect(await screen.findByRole("heading", { name: "Sign in with Plex" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/setup\/start$/);
  });
});
