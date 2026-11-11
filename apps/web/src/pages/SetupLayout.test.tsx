import { Route, Routes } from "react-router";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { LocationDisplay, mockApi, renderWithProviders } from "@/test/render";
import { PlexSetupPage } from "./plexImport/PlexSetupPage";
import { SetupLayout } from "./SetupLayout";
import { SetupStartPage } from "./SetupStartPage";

function renderSetup(route: string) {
  return renderWithProviders(
    <Routes>
      <Route element={<SetupLayout />}>
        <Route path="/setup/start" element={<SetupStartPage />} />
        <Route path="/setup/plex" element={<PlexSetupPage />} />
      </Route>
      <Route path="*" element={<LocationDisplay />} />
    </Routes>,
    { route },
  );
}

describe("SetupLayout", () => {
  it("says how far setup is on how to start, and goes back to Your server", async () => {
    const user = userEvent.setup();
    renderSetup("/setup/start");
    expect(await screen.findByRole("heading", { level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "How far setup is" })).toHaveAttribute("aria-valuenow", "75");
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/setup\/server$/);
  });

  it("shows the Plex import's steps as setup's, with no step count of their own", async () => {
    mockApi({
      "GET /api/import/plex": () => ({ state: "idle", step: null, done: 0, total: 0, error: null, report: null, first_pass_done: false }),
      "GET /api/import/plex/pin": () => ({ signed_in: false }),
    });
    renderSetup("/setup/plex");
    expect(await screen.findByRole("heading", { level: 1, name: "Sign in with Plex" })).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "How far setup is" })).toHaveAttribute("aria-valuenow", "80");
    expect(screen.queryByText(/step 1 of 5/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Moving from Plex" })).not.toBeInTheDocument();
  });
});
