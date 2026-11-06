import { Route, Routes } from "react-router";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ServerStatus } from "@/api/types";
import { LocationDisplay, mockApi, renderWithProviders, testUser } from "@/test/render";
import { SetupPage } from "./SetupPage";

function renderSetup(status: Partial<ServerStatus>, route = "/setup") {
  const requests = mockApi({
    "GET /api/server": () => ({ name: "Home", version: "0.1.0", setup_required: true, ...status }),
    "POST /api/setup": () => ({ user: testUser, token: "t" }),
  });
  renderWithProviders(
    <Routes>
      <Route path="/setup" element={<SetupPage />} />
      <Route path="*" element={<LocationDisplay />} />
    </Routes>,
    { route },
  );
  return requests;
}

const setupPosts = (requests: ReturnType<typeof renderSetup>) => requests.filter((r) => r.method === "POST" && r.path === "/api/setup");

describe("SetupPage", () => {
  it("takes the code from the link the server printed, and checks the password", async () => {
    const user = userEvent.setup();
    const requests = renderSetup({ setup_code_required: true }, "/setup?code=ABCD-EFGH");
    expect(await screen.findByLabelText("Setup code")).toHaveValue("ABCD-EFGH");
    await user.type(screen.getByLabelText("Username"), "owner");
    expect(screen.queryByLabelText(/Confirm/)).not.toBeInTheDocument();

    await user.type(screen.getByLabelText("Password"), "short");
    await user.click(screen.getByRole("button", { name: "Create account" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Use at least 8 characters for the password.");
    expect(setupPosts(requests)).toHaveLength(0);

    // The password can be shown instead of typed twice.
    await user.click(screen.getByRole("button", { name: "Show password" }));
    expect(screen.getByLabelText("Password")).toHaveAttribute("type", "text");
    await user.type(screen.getByLabelText("Password"), " and sweet");
    await user.click(screen.getByRole("button", { name: "Create account" }));
    await waitFor(() =>
      expect(setupPosts(requests).at(-1)?.body).toEqual({ username: "owner", password: "short and sweet", setup_code: "ABCD-EFGH" }),
    );
    expect(await screen.findByTestId("location")).toHaveTextContent("/setup/start");
  });

  it("asks for the code in the log, also from older links", async () => {
    const user = userEvent.setup();
    const requests = renderSetup({ setup_code_required: true }, "/setup#code=ABCD-EFGH");
    expect(await screen.findByLabelText("Setup code")).toHaveValue("ABCD-EFGH");
    await user.type(screen.getByLabelText("Username"), "owner");
    await user.type(screen.getByLabelText("Password"), "correct horse");
    await user.click(screen.getByRole("button", { name: "Create account" }));
    await waitFor(() =>
      expect(setupPosts(requests).at(-1)?.body).toEqual({ username: "owner", password: "correct horse", setup_code: "ABCD-EFGH" }),
    );
  });
});
