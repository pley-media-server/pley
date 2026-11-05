import type { ReactNode } from "react";
import { Route, Routes } from "react-router";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import type { ConsentRequest, OAuthClient, RequestError } from "@/api/oauth";
import type { User } from "@/api/types";
import { apiError, LocationDisplay, mockApi, TestProviders, testQueryClient, testUser, type FakeRequest } from "@/test/render";
import { AuthorizePage, RequestOverPage } from "./AuthorizePage";

const QUERY =
  "response_type=code&client_id=https%3A%2F%2Fpley-media-server.github.io%2Fpley%2Foauth%2Fandroid.json" +
  "&redirect_uri=app.pley.android%3A%2Foauth%2Fcallback&code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM" +
  "&code_challenge_method=S256&scope=library+play&state=a%26b%3Dc";
const ROUTE = `/oauth/authorize?${QUERY}`;
const CONTINUE = "/api/oauth/requests/r1/continue";

const android: OAuthClient = {
  id: "https://pley-media-server.github.io/pley/oauth/android.json",
  host: "pley-media-server.github.io",
  name: "Pley for Android",
  first_party: true,
};
const other: OAuthClient = { id: "https://apps.example.com/pley.json", host: "apps.example.com", name: "Cool Player", first_party: false };

function consent(over: Partial<ConsentRequest> = {}): ConsentRequest {
  return {
    status: "consent",
    id: "r1",
    expires_at: 1_700_000_600,
    client: android,
    redirect: { kind: "app", to: "app.pley.android" },
    scopes: ["library", "play"],
    ...over,
  };
}

interface Server {
  answer?: () => unknown;
  decision?: (req: FakeRequest) => unknown;
  profiles?: User[];
}

function serve({ answer = () => consent(), decision = () => ({ continue_url: CONTINUE }), profiles = [testUser] }: Server = {}) {
  return mockApi({
    "GET /api/server": () => ({ name: "Living Room", version: "1.0.0", setup_required: false }),
    "GET /api/auth/profiles": () => profiles,
    "POST /api/auth/logout": () => undefined,
    "POST /api/oauth/requests": answer,
    "POST /api/oauth/requests/r1": decision,
  });
}

function renderPage({ user = testUser, strict = false }: { user?: User; strict?: boolean } = {}) {
  const client = testQueryClient();
  return render(
    <Routes>
      <Route path="/oauth/authorize" element={<AuthorizePage />} />
      <Route path="*" element={<LocationDisplay />} />
    </Routes>,
    {
      // At the root, as in main.tsx: effects run twice when the page mounts.
      reactStrictMode: strict,
      wrapper: ({ children }: { children: ReactNode }) => (
        <TestProviders route={ROUTE} user={user} client={client}>
          {children}
        </TestProviders>
      ),
    },
  );
}

/** Whether the window has focus and the tab is visible, as the page reads them. */
let focused = false;
let visibility: DocumentVisibilityState = "visible";
let assign: Mock<(url: string) => void>;

beforeEach(() => {
  focused = false;
  visibility = "visible";
  vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
  vi.spyOn(document, "visibilityState", "get").mockImplementation(() => visibility);
  assign = vi.fn();
  vi.stubGlobal("location", { ...window.location, assign });
});

afterEach(() => {
  vi.useRealTimers();
});

/**
 * Timers the test moves by hand; the rest (requests, React) runs as usual.
 * Testing Library's waiting needs real timers: `settle` stands in for it.
 */
function fakeTimers() {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
}

function wait(ms: number) {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
}

function setFocus(on: boolean) {
  focused = on;
  act(() => {
    window.dispatchEvent(new FocusEvent(on ? "focus" : "blur"));
  });
}

function setVisible(on: boolean) {
  visibility = on ? "visible" : "hidden";
  act(() => {
    document.dispatchEvent(new Event("visibilitychange"));
  });
}

/** Lets answers to requests arrive while timers are fake. */
async function settle() {
  for (let i = 0; i < 20; i++) {
    await act(async () => {
      await Promise.resolve();
    });
  }
}

/** The page has been seen for a whole second: Allow can be pressed. */
function attend() {
  fakeTimers();
  setFocus(true);
  wait(1000);
  vi.useRealTimers();
}

async function consentShown() {
  await screen.findByText("on Living Room");
  return { allow: screen.getByRole("button", { name: "Allow" }), deny: screen.getByRole("button", { name: "Don't allow" }) };
}

const posts = (requests: FakeRequest[], path: string) => requests.filter((r) => r.method === "POST" && r.path === path);

describe("AuthorizePage", () => {
  it("hands the server its query once, even under StrictMode", async () => {
    const requests = serve();
    renderPage({ strict: true });

    await consentShown();
    expect(posts(requests, "/api/oauth/requests")).toHaveLength(1);
    expect(posts(requests, "/api/oauth/requests")[0]?.body).toEqual({ query: QUERY });
  });

  it("names pley's own apps, with a badge", async () => {
    serve();
    const { container } = renderPage();

    expect(await screen.findByRole("heading", { name: "Pley for Android wants to use your Pley account" })).toBeInTheDocument();
    expect(screen.getByText("Pley's own app")).toBeInTheDocument();
    expect(screen.queryByText(/calls itself/)).not.toBeInTheDocument();
    expect(screen.queryByText("This app isn't one of Pley's own")).not.toBeInTheDocument();
    expect(container.querySelector("img")).toBeNull();
  });

  it("names other apps by their address first, and only repeats what they call themselves", async () => {
    serve({ answer: () => consent({ client: other, redirect: { kind: "web", to: "apps.example.com" } }) });
    const { container } = renderPage();

    expect(await screen.findByRole("heading", { name: "apps.example.com wants to use your Pley account" })).toBeInTheDocument();
    expect(screen.getByText("It calls itself “Cool Player” (Pley can't check that name)")).toBeInTheDocument();
    expect(screen.getByText("This app isn't one of Pley's own")).toBeInTheDocument();
    expect(screen.queryByText("Pley's own app")).not.toBeInTheDocument();
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText("the website apps.example.com")).toBeInTheDocument();
    expect(screen.queryByText("Only allow this if you just started signing in to the app.")).not.toBeInTheDocument();
  });

  it("says what the app may do, and what it never can", async () => {
    serve({ answer: () => consent({ scopes: ["library", "play", "playlists"] }) });
    renderPage();

    await consentShown();
    const rows = screen.getAllByRole("listitem").map((li) => li.textContent);
    expect(rows).toEqual([
      "See Admin's libraries, playlists and watch history.",
      "Play titles, and keep Admin's place, watched marks, ratings, track choices and library order.",
      "Make, change and delete Admin's playlists.",
    ]);
    expect(
      screen.getByText("It can't change your password, sign-in settings or devices, and it never gets administrator rights."),
    ).toBeInTheDocument();
    expect(screen.getByText("Your administrator rights stay in Pley's web app.")).toBeInTheDocument();
    expect(screen.getByText("an app on this device")).toBeInTheDocument();
    expect(screen.getByText("Only allow this if you just started signing in to the app.")).toBeInTheDocument();
  });

  it("says what requests let an app do, and which administrator right it gets", async () => {
    serve({ answer: () => consent({ scopes: ["library", "requests", "approvals"] }) });
    renderPage();

    await consentShown();
    const rows = screen.getAllByRole("listitem").map((li) => li.textContent);
    expect(rows).toEqual([
      "See Admin's libraries, playlists and watch history.",
      "Ask for movies and shows the libraries don't have, and follow Admin's requests.",
      "Approve and decline what people ask for, where the account is an administrator.",
    ]);
    expect(screen.getByText("It can't change your password, sign-in settings or devices.")).toBeInTheDocument();
    expect(
      screen.getByText("Of your administrator rights it gets only approving and declining requests: the others stay in Pley's web app."),
    ).toBeInTheDocument();
  });

  it("says Pley's own app allowed activity sees and stops everyone's streams", async () => {
    serve({ answer: () => consent({ scopes: ["library", "play", "activity"] }) });
    renderPage();

    await consentShown();
    const rows = screen.getAllByRole("listitem").map((li) => li.textContent);
    expect(rows[2]).toBe("See what's playing and stop it: Admin's streams, and everyone's where the account is an administrator.");
    expect(screen.getByText("It can't change your password, sign-in settings or devices.")).toBeInTheDocument();
    expect(
      screen.getByText("Of your administrator rights it gets only seeing and stopping everyone's streams: the others stay in Pley's web app."),
    ).toBeInTheDocument();
  });

  it("gives another publisher's app allowed activity only the profile's own streams", async () => {
    serve({ answer: () => consent({ client: other, scopes: ["library", "activity"], redirect: { kind: "web", to: "apps.example.com" } }) });
    renderPage();

    await consentShown();
    expect(screen.getByText("See what's playing and stop it: Admin's streams.")).toBeInTheDocument();
    expect(
      screen.getByText("It can't change your password, sign-in settings or devices, and it never gets administrator rights."),
    ).toBeInTheDocument();
    expect(screen.getByText("Your administrator rights stay in Pley's web app.")).toBeInTheDocument();
  });

  it("leaves out the administrator line for others", async () => {
    serve({ answer: () => consent({ scopes: ["library", "profiles"], redirect: { kind: "loopback", to: "127.0.0.1" } }) });
    renderPage({ user: { ...testUser, name: "Kid", is_admin: false } });

    await consentShown();
    expect(screen.getByText("See Kid's libraries, playlists and watch history.")).toBeInTheDocument();
    expect(screen.getByText("Switch to the household's other profiles. Profiles with a PIN still ask for it.")).toBeInTheDocument();
    expect(screen.queryByText("Your administrator rights stay in Pley's web app.")).not.toBeInTheDocument();
    expect(screen.getByText("an app on this computer")).toBeInTheDocument();
    expect(screen.getByText("Only allow this if you just started signing in to the app.")).toBeInTheDocument();
  });

  it("lets Allow be pressed only after the page has been seen for a whole second", async () => {
    serve();
    renderPage();
    const { allow, deny } = await consentShown();

    expect(deny).toHaveFocus();
    expect(allow).toBeDisabled();
    fakeTimers();

    // The window gets the focus: a whole second from then.
    setFocus(true);
    wait(999);
    expect(allow).toBeDisabled();
    wait(1);
    expect(allow).toBeEnabled();

    // Leaving the window starts it over.
    setFocus(false);
    expect(allow).toBeDisabled();
    setFocus(true);
    wait(500);
    expect(allow).toBeDisabled();

    // So does hiding the tab.
    setVisible(false);
    wait(1000);
    expect(allow).toBeDisabled();
    setVisible(true);
    wait(999);
    expect(allow).toBeDisabled();
    wait(1);
    expect(allow).toBeEnabled();
  });

  // Real time from the page's first render: the timer it starts as it
  // appears is the one these check.
  it("keeps Allow off while the window has no focus", async () => {
    serve();
    renderPage();
    const { allow } = await consentShown();
    await new Promise((resolve) => setTimeout(resolve, 1200));
    expect(allow).toBeDisabled();
  });

  it("turns Allow on a second after it appears in a focused window", async () => {
    focused = true;
    serve();
    renderPage();
    const { allow } = await consentShown();
    expect(allow).toBeDisabled();
    await waitFor(() => expect(allow).toBeEnabled(), { timeout: 3000 });
  });

  it("allows, goes back to the app, and offers a link if the browser stays", async () => {
    const requests = serve();
    renderPage();
    const { allow } = await consentShown();
    attend();

    fakeTimers();
    fireEvent.click(allow);
    await settle();
    expect(screen.getByRole("heading", { name: "Returning to Pley for Android…" })).toBeInTheDocument();
    expect(posts(requests, "/api/oauth/requests/r1").map((r) => r.body)).toEqual([{ allow: true }]);
    expect(assign).toHaveBeenCalledExactlyOnceWith(CONTINUE);

    wait(2999);
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    wait(1);
    expect(screen.getByRole("link", { name: "Open Pley for Android" })).toHaveAttribute("href", CONTINUE);

    // Only while the page is seen.
    setVisible(false);
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });

  it("doesn't allow, and goes back the same way", async () => {
    const requests = serve();
    renderPage();
    const { deny } = await consentShown();

    await userEvent.setup().click(deny);
    expect(await screen.findByRole("heading", { name: "Returning to Pley for Android…" })).toBeInTheDocument();
    expect(posts(requests, "/api/oauth/requests/r1").map((r) => r.body)).toEqual([{ allow: false }]);
    expect(assign).toHaveBeenCalledExactlyOnceWith(CONTINUE);
  });

  it("refuses requests it can't send back, with no way back", async () => {
    serve({ answer: () => apiError(400, "bad_request", "this app didn't list that return address") });
    renderPage();

    expect(await screen.findByRole("heading", { name: "This app can't sign in here" })).toBeInTheDocument();
    expect(screen.getByText("This app didn't list that return address")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(assign).not.toHaveBeenCalled();
  });

  it("sends other mistakes back to the app, but only on a click", async () => {
    const error: RequestError = {
      status: "error",
      error: "invalid_scope",
      message: "this app can't ask for openid",
      back_url: "/api/oauth/requests/r1/continue",
      client: other,
      redirect: { kind: "web", to: "apps.example.com" },
    };
    serve({ answer: () => error });
    renderPage();

    expect(await screen.findByText("This app can't ask for openid")).toBeInTheDocument();
    // Where the click leads is shown before it.
    expect(screen.getByText("from apps.example.com")).toBeInTheDocument();
    expect(screen.getByText("Back to the app goes to:")).toBeInTheDocument();
    expect(screen.getByText("the website apps.example.com")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Allow" })).not.toBeInTheDocument();
    expect(assign).not.toHaveBeenCalled();
    await userEvent.setup().click(screen.getByRole("button", { name: "Back to the app" }));
    expect(assign).toHaveBeenCalledExactlyOnceWith("/api/oauth/requests/r1/continue");
  });

  it("says when the request is over", async () => {
    serve({ decision: () => apiError(404, "not_found", "this request is over: start again from the app or the TV") });
    renderPage();
    const { deny } = await consentShown();

    await userEvent.setup().click(deny);
    expect(await screen.findByRole("alert")).toHaveTextContent("This request is over: start again from the app");
    expect(screen.queryByRole("button", { name: "Allow" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Don't allow" })).not.toBeInTheDocument();
    expect(assign).not.toHaveBeenCalled();
  });

  it("says a request is over where a late link back to the app leads", () => {
    const client = testQueryClient();
    render(<RequestOverPage />, {
      wrapper: ({ children }: { children: ReactNode }) => (
        <TestProviders route="/oauth/over" client={client}>
          {children}
        </TestProviders>
      ),
    });
    expect(screen.getByRole("heading", { name: "This request is over" })).toBeInTheDocument();
    expect(screen.getByText("Start again from the app.")).toBeInTheDocument();
  });

  it("comes back to the whole request from another profile", async () => {
    serve({ profiles: [testUser, { ...testUser, id: 2, name: "Kid" }] });
    renderPage();
    await consentShown();

    await userEvent.setup().click(await screen.findByRole("button", { name: "Use another profile" }));
    expect(screen.getByTestId("location")).toHaveTextContent(`/profiles?next=${encodeURIComponent(ROUTE)}`);
  });

  it("signs out and comes back to the whole request", async () => {
    const requests = serve();
    renderPage();
    await consentShown();

    // A single profile has no other to use.
    expect(screen.queryByRole("button", { name: "Use another profile" })).not.toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "Not you? Sign out" }));
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(`/login?next=${encodeURIComponent(ROUTE)}`));
    expect(posts(requests, "/api/auth/logout")).toHaveLength(1);
  });
});
