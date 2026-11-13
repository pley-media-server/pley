import { Route, Routes } from "react-router";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { User } from "@/api/types";
import { rememberProfile } from "@/lib/lastProfile";
import { apiError, LocationDisplay, mockApi, renderWithProviders, testSetup, testUser, twoStepSetupAnswer } from "@/test/render";
import { LoginPage } from "./LoginPage";

interface Server {
  setupRequired?: boolean;
  signedIn?: boolean;
  profiles?: User[];
  /** Who signing in signs in as. */
  user?: User;
  sso?: { name: string; start: string };
}

const AUTHENTIK = { name: "Authentik", start: "https://pley.example.com/api/auth/sso/start" };

const RECOVERY_CODES = Array.from({ length: 10 }, (_, i) => `${i}aaa-bbbb-cccc-dddd`);
/** What the server shows once the first secret got too many wrong codes. */
const NEW_SETUP = {
  ...testSetup,
  secret: "KRSXG5CTMVRXEZLU",
  uri: "otpauth://totp/Pley:admin?secret=KRSXG5CTMVRXEZLU",
  id: "7RZCN3W2HLMQ5DKAXVBT6YPGEU",
};

function serve({ setupRequired = false, signedIn = false, profiles = [testUser], user = testUser, sso }: Server = {}) {
  /** Wrong codes while setting up: the third starts over with a new secret. */
  let wrongSetupCodes = 0;
  return mockApi({
    "GET /api/server": () => ({ name: "Living Room", version: "1.0.0", setup_required: setupRequired, sso: sso ?? null }),
    "GET /api/auth/me": () => (signedIn ? testUser : apiError(401, "unauthorized", "not signed in")),
    "POST /api/auth/login": (req) => {
      const { password, code } = req.body as { password: string; code?: string };
      if (password === "too many") return apiError(429, "too_many_requests", "too many attempts, wait a minute");
      // The server requires two-step sign-in, and this account doesn't use it yet.
      if (password === "required") {
        if (!code) return twoStepSetupAnswer();
        const secret = wrongSetupCodes >= 3 ? NEW_SETUP : testSetup;
        if (code !== (secret === testSetup ? "123456" : "654321")) {
          wrongSetupCodes++;
          return wrongSetupCodes === 3
            ? twoStepSetupAnswer(NEW_SETUP, "that QR code doesn't work any more: add Pley to your authenticator app again with this new one")
            : twoStepSetupAnswer(secret, "that code didn't work: check that your phone's clock is right, and enter the newest code");
        }
        return { user: testUser, recovery_codes: RECOVERY_CODES };
      }
      if (password === "two-step") {
        if (!code) return apiError(401, "two_step", "enter the 6-digit code from your authenticator app");
        if (code !== "123456" && code !== "7k2m-q9xd-4hpt-3vwr") {
          return apiError(401, "two_step", "that code didn't work: enter the newest code from your authenticator app");
        }
        return { user: testUser };
      }
      return password === "open-sesame" ? { user, token: "t" } : apiError(401, "unauthorized", "invalid credentials");
    },
    "GET /api/auth/profiles": () => profiles,
  });
}

function renderLogin(route = "/login?next=%2Flibrary%2F2") {
  return renderWithProviders(
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route path="/setup" element={<p>Setup</p>} />
      <Route path="*" element={<LocationDisplay />} />
    </Routes>,
    { route },
  );
}

async function signIn(password: string) {
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText("Username"), "  admin ");
  await user.type(screen.getByLabelText("Password"), password);
  await user.click(screen.getByRole("button", { name: "Sign in" }));
}

describe("LoginPage", () => {
  afterEach(() => {
    delete (window as { __PLEY_APP__?: unknown }).__PLEY_APP__;
  });

  it("offers single sign-on when the server has it, back to the page asked for", async () => {
    serve({ sso: AUTHENTIK });
    renderLogin();
    const link = await screen.findByRole("link", { name: "Sign in with Authentik" });
    expect(link).toHaveAttribute("href", "https://pley.example.com/api/auth/sso/start?next=%2Flibrary%2F2");
  });

  it("says why a single sign-on didn't sign in", async () => {
    serve({ sso: AUTHENTIK });
    renderLogin("/login?sso=unknown");
    expect(await screen.findByRole("alert")).toHaveTextContent("No account here is linked to that Authentik account");
  });

  it("has no single sign-on button when the server has none", async () => {
    serve();
    renderLogin();
    expect(await screen.findByText("to Living Room")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /Sign in with/ })).not.toBeInTheDocument();
  });

  it("sends single sign-on to the browser from the apps' windows", async () => {
    // The desktop and Android apps keep their windows on the server's pages.
    (window as { __PLEY_APP__?: unknown }).__PLEY_APP__ = "desktop";
    serve({ sso: AUTHENTIK });
    renderLogin();
    expect(await screen.findByText("To sign in with Authentik, open Pley in your browser.")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Sign in with Authentik" })).not.toBeInTheDocument();
  });

  it("signs in and goes where the user was headed", async () => {
    const requests = serve();
    renderLogin();

    expect(await screen.findByText("to Living Room")).toBeInTheDocument();
    await signIn("open-sesame");

    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
    expect(requests.find((r) => r.method === "POST")?.body).toEqual({ username: "admin", password: "open-sesame" });
  });

  it("says when signing in is for letting an app in", async () => {
    serve();
    const authorize = "/oauth/authorize?response_type=code&client_id=https%3A%2F%2Fapps.example.com%2Fpley.json&state=a%26b";
    renderLogin(`/login?next=${encodeURIComponent(authorize)}`);

    expect(await screen.findByText("to let an app use Living Room")).toBeInTheDocument();
    await signIn("open-sesame");
    expect(await screen.findByTestId("location")).toHaveTextContent(authorize);
  });

  it("says when signing in is for letting a TV in", async () => {
    serve();
    renderLogin(`/login?next=${encodeURIComponent("/link?code=BCDFGHJK")}`);

    expect(await screen.findByText("to let your TV use Living Room")).toBeInTheDocument();
    await signIn("open-sesame");
    expect(await screen.findByTestId("location")).toHaveTextContent("/link?code=BCDFGHJK");
  });

  it("lets a household pick a profile first", async () => {
    serve({ profiles: [testUser, { ...testUser, id: 2, name: "Kid" }] });
    renderLogin();

    await signIn("open-sesame");
    expect(await screen.findByTestId("location")).toHaveTextContent("/profiles?next=%2Flibrary%2F2");
  });

  it("goes straight in as the profile this browser was last used as, when it has no PIN", async () => {
    rememberProfile(testUser.id);
    serve({ profiles: [testUser, { ...testUser, id: 2, name: "Kid" }] });
    renderLogin();

    await signIn("open-sesame");
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
  });

  it("still lets a household pick when that profile has a PIN, or another was used last", async () => {
    const household = [testUser, { ...testUser, id: 2, name: "Kid" }];
    rememberProfile(testUser.id);
    serve({ profiles: household, user: { ...testUser, has_pin: true } });
    const first = renderLogin();
    await signIn("open-sesame");
    expect(await screen.findByTestId("location")).toHaveTextContent("/profiles?next=%2Flibrary%2F2");
    first.unmount();

    rememberProfile(2);
    serve({ profiles: household });
    renderLogin();
    await signIn("open-sesame");
    expect(await screen.findByTestId("location")).toHaveTextContent("/profiles?next=%2Flibrary%2F2");
  });

  it("asks for the code from the authenticator app, and signs in with it", async () => {
    const requests = serve();
    renderLogin();

    await signIn("two-step");
    expect(await screen.findByRole("heading", { name: "Two-step sign-in" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    const user = userEvent.setup();
    const code = screen.getByLabelText("Code");
    expect(code).toHaveAttribute("autocomplete", "one-time-code");

    // A wrong code says so; a whole one goes as soon as it's typed.
    await user.type(code, "999999");
    expect(await screen.findByRole("alert")).toHaveTextContent("That code didn't work");
    await user.clear(code);
    await user.type(code, "123 456");
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
    expect(requests.filter((r) => r.method === "POST").at(-1)?.body).toEqual({
      username: "admin",
      password: "two-step",
      code: "123456",
    });
  });

  it("takes a recovery code instead", async () => {
    serve();
    renderLogin();

    await signIn("two-step");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Use a recovery code" }));
    const code = screen.getByLabelText("Recovery code");
    const submit = screen.getByRole("button", { name: "Sign in" });
    await user.type(code, "7k2m-q9xd");
    expect(submit).toBeDisabled();
    await user.type(code, "-4hpt-3vwr");
    await user.click(submit);
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
  });

  it("takes an app's code pasted with a space in it", async () => {
    serve();
    renderLogin();

    await signIn("two-step");
    const user = userEvent.setup();
    await user.click(await screen.findByLabelText("Code"));
    await user.paste("123 456");
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
  });

  it("sets two-step sign-in up right after the password when the server requires it", async () => {
    const requests = serve();
    renderLogin();

    await signIn("required");
    expect(await screen.findByRole("heading", { name: "Set up two-step sign-in" })).toBeInTheDocument();
    expect(screen.getByText(/Living Room asks everyone for a code/)).toBeInTheDocument();
    // Being asked to set it up isn't an error.
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("img", { name: /QR code/ })).toBeInTheDocument();
    expect(screen.getByText("JBSW Y3DP EHPK 3PXP")).toBeInTheDocument();
    expect(screen.getByText(/Google Authenticator, Microsoft Authenticator/)).toBeInTheDocument();

    const user = userEvent.setup();
    const code = screen.getByLabelText("Code from the app");
    expect(code).toHaveAttribute("autocomplete", "one-time-code");
    await user.type(code, "999999");
    expect(await screen.findByRole("alert")).toHaveTextContent("That code didn't work");
    await user.clear(code);
    await user.type(code, "123 456");

    // The recovery codes come first, once; then where the person was going.
    expect(await screen.findByRole("heading", { name: "Save your recovery codes" })).toBeInTheDocument();
    const list = screen.getByRole("list", { name: "Recovery codes" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(10);
    expect(screen.queryByTestId("location")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "I've saved them" }));
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");

    // The password was typed once, and went with every try.
    const logins = requests.filter((r) => r.method === "POST" && r.path === "/api/auth/login");
    expect(logins.map((r) => r.body)).toEqual([
      { username: "admin", password: "required" },
      // With the id of the secret shown: only it gets that secret again.
      { username: "admin", password: "required", code: "999999", setup_id: testSetup.id },
      { username: "admin", password: "required", code: "123456", setup_id: testSetup.id },
    ]);
  });

  it("shows a new QR code when the last one stops working", async () => {
    const requests = serve();
    renderLogin();

    await signIn("required");
    const user = userEvent.setup();
    for (const wrong of ["111111", "222222", "333333"]) {
      const code = await screen.findByLabelText("Code from the app");
      await user.clear(code);
      await user.type(code, wrong);
    }
    expect(await screen.findByText("KRSX G5CT MVRX EZLU")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("That QR code doesn't work any more");
    // A new secret starts the code over.
    const code = screen.getByLabelText("Code from the app");
    expect(code).toHaveValue("");
    await user.type(code, "654321");
    expect(await screen.findByRole("heading", { name: "Save your recovery codes" })).toBeInTheDocument();
    const last = requests.filter((r) => r.method === "POST" && r.path === "/api/auth/login").at(-1);
    expect(last?.body).toMatchObject({ code: "654321", setup_id: NEW_SETUP.id });
  });

  it("goes back to the password from setting it up", async () => {
    serve();
    renderLogin();

    await signIn("required");
    await userEvent.setup().click(await screen.findByRole("button", { name: "Back" }));
    expect(await screen.findByLabelText("Password")).toHaveValue("");
  });

  it("goes back to the password", async () => {
    serve();
    renderLogin();

    await signIn("two-step");
    await userEvent.setup().click(await screen.findByRole("button", { name: "Back" }));
    expect(await screen.findByLabelText("Password")).toHaveValue("");
    expect(screen.getByLabelText("Username")).toHaveValue("  admin ");
  });

  it("says when the password is wrong", async () => {
    serve();
    renderLogin();

    await signIn("nope");
    expect(await screen.findByRole("alert")).toHaveTextContent("Wrong username or password.");
    expect(screen.getByRole("button", { name: "Sign in" })).toBeEnabled();
  });

  it("passes on the server's explanation of other refusals", async () => {
    serve();
    renderLogin();

    await signIn("too many");
    expect(await screen.findByRole("alert")).toHaveTextContent("Too many attempts, wait a minute");
  });

  it("skips the form when already signed in", async () => {
    serve({ signedIn: true });
    renderLogin();
    expect(await screen.findByTestId("location")).toHaveTextContent("/library/2");
  });

  it("sends a new server to setup", async () => {
    serve({ setupRequired: true });
    renderLogin();
    expect(await screen.findByText("Setup")).toBeInTheDocument();
  });

  it("ignores redirects to other sites", async () => {
    serve();
    renderLogin("/login?next=%2F%5Cevil.example");

    await signIn("open-sesame");
    expect((await screen.findByTestId("location")).textContent).toBe("/");
  });
});
