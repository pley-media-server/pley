import { Route, Routes } from "react-router";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { LocationDisplay, mockApi, renderWithProviders, testUser } from "@/test/render";
import { PlexSetupPage } from "./plexImport/PlexSetupPage";
import { SetupLayout, SetupStep } from "./SetupLayout";
import { SetupStartPage } from "./SetupStartPage";

function renderSetup(route: string, user = testUser) {
  return renderWithProviders(
    <Routes>
      <Route element={<SetupLayout />}>
        <Route path="/setup/start" element={<SetupStartPage />} />
        <Route path="/setup/plex" element={<PlexSetupPage />} />
        <Route path="/setup/library" element={<SetupStep progress={90} title="Your libraries">{null}</SetupStep>} />
      </Route>
      <Route path="*" element={<LocationDisplay />} />
    </Routes>,
    { route, user },
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

  it("sends anyone but an administrator home from how to start", async () => {
    renderSetup("/setup/start", { ...testUser, is_admin: false });
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/$/);
  });

  it("puts the focus on a step's title as it comes in, not on the page", async () => {
    const user = userEvent.setup();
    renderSetup("/setup/start");
    await user.click(await screen.findByRole("link", { name: /^Start fresh/ }));
    expect(await screen.findByRole("heading", { name: "Your libraries" })).toHaveFocus();
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
