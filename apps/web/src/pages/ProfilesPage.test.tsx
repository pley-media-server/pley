import { Route, Routes } from "react-router";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { User } from "@/api/types";
import { apiError, type FakeRequest, LocationDisplay, mockApi, renderWithProviders, testSetup, testUser, twoStepSetupAnswer } from "@/test/render";
import { challenge, ProfilesPage } from "./ProfilesPage";

const me: User = { ...testUser, id: 1, name: "Sam", is_admin: false };
const kid: User = { ...testUser, id: 2, name: "Kid", is_admin: false };
const locked: User = { ...testUser, id: 3, name: "Alex", is_admin: false, has_pin: true };
const admin: User = { ...testUser, id: 4, name: "Owner", is_admin: true };
const guarded: User = { ...testUser, id: 5, name: "Boss", is_admin: true, two_step: true };
/** Its PIN is locked after too many wrong ones. */
const stuck: User = { ...testUser, id: 6, name: "Robin", is_admin: false, has_pin: true };
/** Listed without a PIN, which it has since. */
const lately: User = { ...testUser, id: 7, name: "Jo", is_admin: false };

function serve() {
  return mockApi({
    "GET /api/auth/profiles": () => [me, kid, locked, admin, guarded, stuck, lately],
    "POST /api/auth/switch": (req) => {
      const body = req.body as { user_id: number; pin?: string; password?: string; code?: string };
      if (body.user_id === guarded.id) {
        return body.password === "secret" && body.code === "123456"
          ? { user: guarded, token: "t" }
          : apiError(401, "two_step", "that code didn't work");
      }
      if (body.user_id === locked.id && body.pin !== "2468") return apiError(403, "wrong_pin", "wrong PIN");
      if (body.user_id === stuck.id) return apiError(403, "pin_locked", "this profile's PIN is locked after too many wrong tries");
      if (body.user_id === lately.id && body.pin !== "1357") return apiError(403, "wrong_pin", body.pin ? "wrong PIN" : "this profile is protected by a PIN");
      if (body.user_id === admin.id && body.password !== "secret") return apiError(403, "forbidden", "wrong password");
      return { user: [kid, locked, admin, lately].find((u) => u.id === body.user_id), token: "t" };
    },
  });
}

/** The last profile switch: queries refetched after it may come later. */
function lastSwitch(requests: FakeRequest[]) {
  return requests.filter((r) => r.method === "POST" && r.path === "/api/auth/switch").at(-1);
}

function renderProfiles() {
  return renderWithProviders(
    <Routes>
      <Route path="/profiles" element={<ProfilesPage />} />
      <Route path="*" element={<LocationDisplay />} />
    </Routes>,
    { route: "/profiles?next=%2Flibrary%2F2", user: me },
  );
}

describe("ProfilesPage", () => {
  it("switches to a profile without a PIN", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderProfiles();

    await user.click(await screen.findByRole("button", { name: "Kid" }));
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
    expect(lastSwitch(requests)?.body).toEqual({ user_id: kid.id });
  });

  it("goes on as the current profile without asking the server", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderProfiles();

    await user.click(await screen.findByRole("button", { name: "Sam" }));
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
    expect(requests.filter((r) => r.method === "POST")).toHaveLength(0);
  });

  it("asks for the PIN of a locked profile and starts over after a wrong one", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderProfiles();

    await user.click(await screen.findByRole("button", { name: "Alex" }));
    expect(screen.getByText("Enter the profile PIN")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Switch" })).toBeDisabled();

    await user.keyboard("1235");
    await user.keyboard("{Backspace}");
    expect(screen.getByLabelText("3 digits entered")).toBeInTheDocument();
    await user.keyboard("4{Enter}");
    expect(await screen.findByText("Wrong PIN")).toHaveAttribute("role", "alert");
    expect(screen.getByLabelText("0 digits entered")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "2" }));
    await user.click(screen.getByRole("button", { name: "4" }));
    await user.click(screen.getByRole("button", { name: "6" }));
    await user.click(screen.getByRole("button", { name: "8" }));
    await user.click(screen.getByRole("button", { name: "Switch" }));

    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
    expect(requests.filter((r) => r.method === "POST").map((r) => r.body)).toEqual([
      { user_id: locked.id, pin: "1234" },
      { user_id: locked.id, pin: "2468" },
    ]);
  });

  it("stops the pad once the PIN is locked", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderProfiles();

    await user.click(await screen.findByRole("button", { name: "Robin" }));
    await user.keyboard("1234{Enter}");
    expect(await screen.findByText(/PIN is locked/)).toHaveAttribute("role", "alert");
    expect(screen.getByRole("button", { name: "5" })).toBeDisabled();
    await user.keyboard("5678{Enter}");
    expect(requests.filter((r) => r.method === "POST")).toHaveLength(1);
    // Another profile's pad starts as usual.
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Alex" }));
    expect(screen.getByRole("button", { name: "5" })).toBeEnabled();
  });

  it("asks for the PIN of a profile that has one since the list was loaded", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderProfiles();

    await user.click(await screen.findByRole("button", { name: "Jo" }));
    expect(await screen.findByText("Enter the profile PIN")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeEmptyDOMElement();
    await user.keyboard("1357{Enter}");
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
    expect(requests.filter((r) => r.method === "POST" && r.path === "/api/auth/switch").map((r) => r.body)).toEqual([
      { user_id: lately.id },
      { user_id: lately.id, pin: "1357" },
    ]);
  });

  it("goes back to the profiles from the PIN pad", async () => {
    const user = userEvent.setup();
    serve();
    renderProfiles();

    await user.click(await screen.findByRole("button", { name: "Alex" }));
    await user.keyboard("{Escape}");
    expect(screen.getByRole("heading", { name: "Who's watching?" })).toBeInTheDocument();
  });

  it("asks for the password of an administrator", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderProfiles();

    await user.click(await screen.findByRole("button", { name: "Owner" }));
    await user.type(screen.getByLabelText("Password"), "guess");
    await user.click(screen.getByRole("button", { name: "Switch" }));
    expect(await screen.findByText("Wrong password")).toHaveAttribute("role", "alert");

    await user.clear(screen.getByLabelText("Password"));
    await user.type(screen.getByLabelText("Password"), "secret");
    await user.click(screen.getByRole("button", { name: "Switch" }));
    await waitFor(() => expect(lastSwitch(requests)?.body).toEqual({ user_id: admin.id, password: "secret" }));
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
  });

  it("asks for the code too when the profile has two-step sign-in", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderProfiles();

    await user.click(await screen.findByRole("button", { name: "Boss" }));
    expect(screen.getByText(/the code from its authenticator app/)).toBeInTheDocument();
    await user.type(screen.getByLabelText("Password"), "secret");
    expect(screen.getByRole("button", { name: "Switch" })).toBeDisabled();
    await user.type(screen.getByLabelText("Code from the authenticator app"), "123456");
    await user.click(screen.getByRole("button", { name: "Switch" }));
    await waitFor(() =>
      expect(lastSwitch(requests)?.body).toEqual({ user_id: guarded.id, password: "secret", code: "123456" }),
    );
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
  });

  it("sets two-step sign-in up for a profile that has to, with its password, when the server requires it", async () => {
    const user = userEvent.setup();
    // Mom has a PIN, but hasn't set up two-step sign-in, which the server requires.
    const mom: User = { ...testUser, id: 6, name: "Mom", is_admin: false, has_pin: true };
    const codes = Array.from({ length: 10 }, (_, i) => `${i}aaa-bbbb-cccc-dddd`);
    const requests = mockApi({
      "GET /api/server": () => ({ name: "Living Room", version: "1.0.0", setup_required: false, two_step_required: true }),
      "GET /api/auth/profiles": () => [me, mom],
      "POST /api/auth/switch": (req) => {
        const body = req.body as { password?: string; code?: string };
        if (body.password !== "mom-pass-1") return apiError(403, "forbidden", "wrong password");
        if (!body.code) return twoStepSetupAnswer();
        if (body.code !== "123456") return twoStepSetupAnswer(undefined, "that code didn't work: check that your phone's clock is right");
        return { user: { ...mom, two_step: true }, recovery_codes: codes };
      },
    });
    renderProfiles();

    // Her password, not her PIN: that's what sets it up.
    await user.click(await screen.findByRole("button", { name: "Mom" }));
    expect(screen.getByText(/requires two-step sign-in, which Mom hasn't set up yet/)).toBeInTheDocument();
    expect(screen.queryByText("Enter the profile PIN")).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("Password"), "mom-pass-1");
    await user.click(screen.getByRole("button", { name: "Switch" }));

    expect(await screen.findByRole("heading", { name: "Set up two-step sign-in for Mom" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    const code = screen.getByLabelText("Code from the app");
    await user.type(code, "999999");
    expect(await screen.findByRole("alert")).toHaveTextContent("That code didn't work");
    await user.clear(code);
    await user.type(code, "123456");

    expect(await screen.findByRole("list", { name: "Recovery codes" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "I've saved them" }));
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
    expect(lastSwitch(requests)?.body).toEqual({
      user_id: mom.id,
      password: "mom-pass-1",
      code: "123456",
      setup_id: testSetup.id,
    });
  });

  it("can skip choosing", async () => {
    const user = userEvent.setup();
    serve();
    renderProfiles();
    await user.click(await screen.findByRole("button", { name: "Continue as Sam" }));
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
  });

  it("says when the profiles can't be loaded, and tries again", async () => {
    const user = userEvent.setup();
    let fail = true;
    mockApi({ "GET /api/auth/profiles": () => (fail ? apiError(500, "internal", "the server is busy") : [me, kid]) });
    renderProfiles();

    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't load the profilesThe server is busy");
    fail = false;
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("button", { name: "Kid" })).toBeInTheDocument();
  });
});

describe("challenge", () => {
  const child: User = { ...testUser, id: 10, is_admin: false, managed: true, has_password: false, max_age: 12 };
  const teen: User = { ...child, id: 11, max_age: 16 };
  const parent: User = { ...testUser, id: 12, is_admin: false, max_age: null };

  it("asks for nothing to switch down, or between equals", () => {
    expect(challenge(child, parent)).toBe("none");
    expect(challenge({ ...child, id: 13 }, child)).toBe("none");
  });

  it("asks for a password, or a PIN, to switch to a profile that can see more", () => {
    expect(challenge(parent, child)).toBe("password");
    expect(challenge({ ...teen, has_pin: true }, child)).toBe("pin");
    // A managed profile has no password: it needs a PIN first.
    expect(challenge(teen, child)).toBe("locked");
  });

  it("counts libraries too", () => {
    const one: User = { ...parent, id: 14, all_libraries: false, library_ids: [1] };
    const both: User = { ...parent, id: 15, all_libraries: false, library_ids: [1, 2] };
    expect(challenge(both, one)).toBe("password");
    expect(challenge(one, both)).toBe("none");
  });

  it("asks a profile that has to set up two-step sign-in for its password, whatever else it has", () => {
    const server = { name: "Den", version: "1.0.0", setup_required: false, two_step_required: true };
    const withPin: User = { ...parent, id: 16, has_pin: true };
    expect(challenge(withPin, parent, server)).toBe("setup");
    expect(challenge({ ...parent, id: 17 }, testUser, server)).toBe("setup");
    // Set up already, or without a password (managed), or not required: as ever.
    expect(challenge({ ...withPin, two_step: true }, parent, server)).toBe("pin");
    expect(challenge(child, parent, server)).toBe("none");
    expect(challenge(withPin, parent, { ...server, two_step_required: false })).toBe("pin");
    expect(challenge(withPin, withPin, server)).toBe("none");
  });
});
