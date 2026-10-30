import type { ReactNode } from "react";
import { Route, Routes } from "react-router";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import type { ConsentRequest } from "@/api/oauth";
import type { User } from "@/api/types";
import { apiError, LocationDisplay, mockApi, TestProviders, testQueryClient, testUser, type FakeRequest } from "@/test/render";
import { LinkPage } from "./LinkPage";

/** The TV's request, as the server answers a code it knows. */
function tvRequest(): ConsentRequest {
  return {
    status: "consent",
    id: "d1",
    expires_at: 1_700_000_600,
    client: {
      id: "https://pley-media-server.github.io/pley/oauth/android.json",
      host: "pley-media-server.github.io",
      name: "Pley for Android",
      first_party: true,
    },
    redirect: { kind: "device", to: "BCDF-GHJK" },
    scopes: ["library", "play"],
  };
}

interface Server {
  verify?: (req: FakeRequest) => unknown;
  decision?: (req: FakeRequest) => unknown;
  profiles?: User[];
}

function serve({ verify = () => tvRequest(), decision = () => ({ done: true }), profiles = [testUser] }: Server = {}) {
  return mockApi({
    "GET /api/server": () => ({ name: "Living Room", version: "1.0.0", setup_required: false }),
    "GET /api/auth/profiles": () => profiles,
    "POST /api/auth/logout": () => undefined,
    "POST /api/oauth/device/verify": verify,
    "POST /api/oauth/requests/d1": decision,
  });
}

function renderPage(route = "/link", { strict = false }: { strict?: boolean } = {}) {
  const client = testQueryClient();
  return render(
    <Routes>
      <Route path="/link" element={<LinkPage />} />
      <Route path="*" element={<LocationDisplay />} />
    </Routes>,
    {
      // At the root, as in main.tsx: effects run twice when the page mounts.
      reactStrictMode: strict,
      wrapper: ({ children }: { children: ReactNode }) => (
        <TestProviders route={route} client={client}>
          {children}
        </TestProviders>
      ),
    },
  );
}

/** Whether the window has focus, as the page reads it (the tab is always visible here). */
let focused = false;
let assign: Mock<(url: string) => void>;

beforeEach(() => {
  focused = false;
  vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
  vi.spyOn(document, "visibilityState", "get").mockImplementation(() => "visible");
  assign = vi.fn();
  vi.stubGlobal("location", { ...window.location, assign });
});

afterEach(() => {
  vi.useRealTimers();
});

/** The page has been seen for a whole second: Allow can be pressed. */
function attend() {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  focused = true;
  act(() => {
    window.dispatchEvent(new FocusEvent("focus"));
  });
  act(() => {
    vi.advanceTimersByTime(1000);
  });
  vi.useRealTimers();
}

const URL_CODE = "/link?code=BCDFGHJK";
const HEADING = "Pley for Android wants to use your Pley account";

/** Types `code` (null: the address brought it) and continues; the consent's buttons. */
async function continueWith(code: string | null) {
  const user = userEvent.setup();
  if (code) await user.type(await screen.findByLabelText("Code"), code);
  await user.click(await screen.findByRole("button", { name: "Continue" }));
  await screen.findByRole("heading", { name: HEADING });
  await screen.findByText("on Living Room");
  return { allow: screen.getByRole("button", { name: "Allow" }), deny: screen.getByRole("button", { name: "Don't allow" }) };
}

const posts = (requests: FakeRequest[], path: string) => requests.filter((r) => r.method === "POST" && r.path === path);

describe("LinkPage", () => {
  it("fills in a code from the address but never sends it by itself", async () => {
    const requests = serve();
    renderPage("/link?code=bcdf-ghjk", { strict: true });

    const input = await screen.findByLabelText("Code");
    expect(input).toHaveValue("BCDF-GHJK");
    expect(input).not.toHaveFocus();
    expect(screen.getByRole("heading", { name: "Sign in on your TV" })).toBeInTheDocument();
    expect(screen.getByText("Check that your TV shows this code, then continue.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();

    // Time for anything that would send it.
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
    expect(posts(requests, "/api/oauth/device/verify")).toHaveLength(0);
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();

    // Changed, it's a code being typed.
    await userEvent.setup().type(input, "{Backspace}");
    expect(screen.getByText("Enter the code your TV shows.")).toBeInTheDocument();
  });

  it("says which device is asking: what it says it is, and where from", async () => {
    const kodi: ConsentRequest = {
      ...tvRequest(),
      client: { id: "https://pley-media-server.github.io/pley/oauth/kodi.json", host: "pley-media-server.github.io", name: "Pley for Kodi", first_party: true },
      redirect: { kind: "device", to: "BCDF-GHJK", device: "Kodi 21.3; Windows", address: "192.168.1.20" },
    };
    serve({ verify: () => kodi });
    renderPage(URL_CODE);
    await userEvent.setup().click(await screen.findByRole("button", { name: "Continue" }));
    await screen.findByRole("heading", { name: "Pley for Kodi wants to use your Pley account" });
    expect(screen.getByText("It says it is Kodi 21.3; Windows, at 192.168.1.20.")).toBeInTheDocument();
  });

  it("names a device that says nothing by its address alone", async () => {
    serve({ verify: () => ({ ...tvRequest(), redirect: { kind: "device", to: "BCDF-GHJK", device: null, address: "10.0.0.7" } }) });
    renderPage(URL_CODE);
    await continueWith(null);
    expect(screen.getByText("It says it is a device, at 10.0.0.7.")).toBeInTheDocument();
  });

  it("asks for the rest of a code the address cut short", async () => {
    serve();
    renderPage("/link?code=bcd");

    const input = await screen.findByLabelText("Code");
    expect(input).toHaveValue("BCD");
    expect(input).toHaveFocus();
    expect(screen.getByText("Enter the code your TV shows.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  });

  it("shows the code as the TV does while it's typed, and continues only with a whole one", async () => {
    const requests = serve();
    renderPage();

    expect(await screen.findByText("Enter the code your TV shows.")).toBeInTheDocument();
    const input = screen.getByLabelText("Code");
    const go = screen.getByRole("button", { name: "Continue" });
    expect(input).toHaveFocus();
    expect(input).toHaveAttribute("autocomplete", "off");
    expect(input).toHaveAttribute("autocapitalize", "characters");
    expect(input).toHaveAttribute("spellcheck", "false");
    expect(input).toHaveAttribute("inputmode", "text");
    expect(input).toHaveAttribute("maxlength", "9");
    expect(go).toBeDisabled();

    const user = userEvent.setup();
    await user.type(input, "bcdf");
    expect(input).toHaveValue("BCDF");
    // Spaces, dashes and digits are left out; the dash comes back after the 4th letter.
    await user.type(input, " g-h2j");
    expect(input).toHaveValue("BCDF-GHJ");
    expect(go).toBeDisabled();
    await user.type(input, "k");
    expect(input).toHaveValue("BCDF-GHJK");
    expect(go).toBeEnabled();
    // No 9th letter.
    await user.type(input, "L");
    expect(input).toHaveValue("BCDF-GHJK");

    // Back to 4 letters, the dash goes too.
    await user.type(input, "{Backspace}{Backspace}{Backspace}{Backspace}");
    expect(input).toHaveValue("BCDF");
    expect(go).toBeDisabled();

    // Letters no code has: Continue waits, and the page says why.
    await user.type(input, "gaij");
    expect(input).toHaveValue("BCDF-GAIJ");
    expect(screen.getByText("Codes don't use A, E, I, O, U or Y: check the code on your TV.")).toBeInTheDocument();
    expect(go).toBeDisabled();
    await user.type(input, "{Enter}");
    expect(posts(requests, "/api/oauth/device/verify")).toHaveLength(0);
  });

  it("checks the code, then asks whether the TV may use the account", async () => {
    const requests = serve();
    renderPage();

    const { allow, deny } = await continueWith("bcdf ghjk");
    expect(posts(requests, "/api/oauth/device/verify").map((r) => r.body)).toEqual([{ user_code: "BCDFGHJK" }]);

    expect(screen.getByText("Pley's own app")).toBeInTheDocument();
    expect(screen.getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "See Admin's libraries, playlists and watch history.",
      "Play titles, and keep Admin's place, watched marks, ratings, track choices and library order.",
    ]);
    expect(screen.getByText("It goes to:")).toBeInTheDocument();
    expect(screen.getByText(/^the TV or device showing/)).toHaveTextContent("the TV or device showing BCDF-GHJK");
    expect(
      screen.getByText(
        "Only allow this if that code is on a screen in front of you right now. Never use a code someone sent you.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("It goes back to:")).not.toBeInTheDocument();
    expect(screen.queryByText("Only allow this if you just started signing in to the app.")).not.toBeInTheDocument();

    // As for apps: Deny has the focus, and Allow waits a whole second.
    expect(deny).toHaveFocus();
    expect(allow).toBeDisabled();
    attend();
    expect(allow).toBeEnabled();
  });

  it("allows the TV and says it's signing in, staying on the page", async () => {
    const requests = serve();
    renderPage(URL_CODE);
    const { allow } = await continueWith(null);
    attend();

    await userEvent.setup().click(allow);
    expect(await screen.findByRole("heading", { name: "Pley for Android on your TV is signing in" })).toBeInTheDocument();
    expect(screen.getByText("You can close this page.")).toBeInTheDocument();
    expect(posts(requests, "/api/oauth/requests/d1").map((r) => r.body)).toEqual([{ allow: true }]);
    expect(assign).not.toHaveBeenCalled();
    expect(screen.queryByTestId("location")).not.toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("takes neither a second Enter nor a held one as the answer", async () => {
    const requests = serve();
    renderPage();
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Code"), "bcdfghjk");

    // Enter sends the code. Pressed again at once, it lands on the question,
    // where "Don't allow" has the focus...
    await user.keyboard("{Enter}");
    const deny = await screen.findByRole("button", { name: "Don't allow" });
    expect(deny).toHaveFocus();
    await user.keyboard("{Enter}");
    // ...and so does Enter held down.
    await user.keyboard("{Enter>3/}");
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
    expect(posts(requests, "/api/oauth/requests/d1")).toHaveLength(0);
    expect(screen.getByRole("heading", { name: HEADING })).toBeInTheDocument();

    // A moment later a key held down still doesn't answer, but a press of its own does.
    await act(() => new Promise((resolve) => setTimeout(resolve, 500)));
    expect(fireEvent.keyDown(deny, { key: "Enter", repeat: true })).toBe(false);
    expect(posts(requests, "/api/oauth/requests/d1")).toHaveLength(0);
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { name: "Not allowed" })).toBeInTheDocument();
    expect(posts(requests, "/api/oauth/requests/d1").map((r) => r.body)).toEqual([{ allow: false }]);
  });

  it("doesn't allow the TV, and says the TV will know", async () => {
    const requests = serve();
    renderPage(URL_CODE);
    const { deny } = await continueWith(null);

    await userEvent.setup().click(deny);
    expect(await screen.findByRole("heading", { name: "Not allowed" })).toBeInTheDocument();
    expect(screen.getByText("Your TV will say so.")).toBeInTheDocument();
    expect(posts(requests, "/api/oauth/requests/d1").map((r) => r.body)).toEqual([{ allow: false }]);
    expect(assign).not.toHaveBeenCalled();
    expect(screen.queryByTestId("location")).not.toBeInTheDocument();
  });

  it.each([
    [400, "bad_request", "that isn't a code from a Pley app: codes are 8 letters, like BCDF-GHJK"],
    [404, "not_found", "no TV is waiting for that code: check the code on your TV, or start again there"],
    [409, "conflict", "that code was already used: start again on your TV"],
    [429, "too_many_requests", "too many wrong codes: wait a few minutes and try again"],
  ])("says why the server turned a code down (%i), and keeps it for another try", async (status, code, message) => {
    let tries = 0;
    const requests = serve({ verify: () => (++tries === 1 ? apiError(status, code, message) : tvRequest()) });
    renderPage();
    const user = userEvent.setup();
    const input = await screen.findByLabelText("Code");
    await user.type(input, "bcdfghjk");
    await user.click(screen.getByRole("button", { name: "Continue" }));

    const shown = message.charAt(0).toUpperCase() + message.slice(1);
    expect(await screen.findByRole("alert")).toHaveTextContent(shown);
    expect(input).toHaveValue("BCDF-GHJK");
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();

    // Typing puts the sentence away; the next try goes through.
    await user.type(input, "{Backspace}");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await user.type(input, "k");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: HEADING })).toBeInTheDocument();
    expect(posts(requests, "/api/oauth/device/verify")).toHaveLength(2);
  });

  it("says when the TV's request is over", async () => {
    serve({ decision: () => apiError(404, "not_found", "this request is over: start again from the app or the TV") });
    renderPage(URL_CODE);
    const { deny } = await continueWith(null);

    await userEvent.setup().click(deny);
    expect(await screen.findByRole("alert")).toHaveTextContent("This request is over: start again on your TV");
    expect(screen.queryByRole("button", { name: "Allow" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Don't allow" })).not.toBeInTheDocument();
  });

  it("comes back to the code from another profile", async () => {
    serve({ profiles: [testUser, { ...testUser, id: 2, name: "Kid" }] });
    renderPage();
    await continueWith("bcdfghjk");

    await userEvent.setup().click(await screen.findByRole("button", { name: "Use another profile" }));
    expect(screen.getByTestId("location")).toHaveTextContent(`/profiles?next=${encodeURIComponent(URL_CODE)}`);
  });

  it("signs out and comes back to the code", async () => {
    const requests = serve();
    renderPage(URL_CODE);
    await continueWith(null);

    await userEvent.setup().click(screen.getByRole("button", { name: "Not you? Sign out" }));
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(`/login?next=${encodeURIComponent(URL_CODE)}`));
    expect(posts(requests, "/api/auth/logout")).toHaveLength(1);
  });
});
