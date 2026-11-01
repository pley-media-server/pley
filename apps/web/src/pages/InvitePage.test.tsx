import { Route, Routes } from "react-router";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { apiError, LocationDisplay, mockApi, renderWithProviders, testSetup, testUser, twoStepSetupAnswer } from "@/test/render";
import { InvitePage } from "./InvitePage";

const RECOVERY_CODES = Array.from({ length: 10 }, (_, i) => `${i}aaa-bbbb-cccc-dddd`);
const dana = { ...testUser, id: 7, username: "dana", name: "dana", is_admin: false, two_step: true };

/** An invitation to Living Room; `required`: the server requires two-step sign-in. */
function serve({ required = false } = {}) {
  return mockApi({
    "GET /api/invites/abc123": () => ({ server_name: "Living Room", expires_at: null }),
    "GET /api/auth/me": () => apiError(401, "unauthorized", "not signed in"),
    "POST /api/invites/abc123/accept": () => (required ? twoStepSetupAnswer() : { user: { ...dana, two_step: false } }),
    "POST /api/auth/login": (req) => {
      const { code } = req.body as { code?: string };
      if (code !== "123456") return twoStepSetupAnswer(undefined, "that code didn't work: check that your phone's clock is right");
      return { user: dana, recovery_codes: RECOVERY_CODES };
    },
  });
}

function renderInvite() {
  return renderWithProviders(
    <Routes>
      <Route path="/invite/:code" element={<InvitePage />} />
      <Route path="*" element={<LocationDisplay />} />
    </Routes>,
    { route: "/invite/abc123" },
  );
}

async function join() {
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText("Username"), "dana");
  await user.type(screen.getByLabelText("Password"), "correct-horse-7");
  await user.click(screen.getByRole("button", { name: "Create account" }));
  return user;
}

describe("InvitePage", () => {
  it("makes the account and goes home", async () => {
    serve();
    renderInvite();

    await join();
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/$/);
  });

  it("sets two-step sign-in up as the person joins a server that requires it", async () => {
    const requests = serve({ required: true });
    renderInvite();

    const user = await join();
    expect(await screen.findByRole("heading", { name: "Set up two-step sign-in" })).toBeInTheDocument();
    expect(screen.getByText(/Your account is made\. Living Room asks everyone/)).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    const code = screen.getByLabelText("Code from the app");
    await user.type(code, "999999");
    expect(await screen.findByRole("alert")).toHaveTextContent("That code didn't work");
    await user.clear(code);
    await user.type(code, "123456");

    expect(await screen.findByRole("heading", { name: "Save your recovery codes" })).toBeInTheDocument();
    expect(within(screen.getByRole("list", { name: "Recovery codes" })).getAllByRole("listitem")).toHaveLength(10);
    await user.click(screen.getByRole("button", { name: "I've saved them" }));
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/$/);

    // The account was made once; signing in to it, with the password typed once, finished.
    expect(requests.filter((r) => r.path === "/api/invites/abc123/accept")).toHaveLength(1);
    expect(requests.filter((r) => r.path === "/api/auth/login").at(-1)?.body).toEqual({
      username: "dana",
      password: "correct-horse-7",
      code: "123456",
      setup_id: testSetup.id,
    });
  });
});
