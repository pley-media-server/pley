import { Route, Routes } from "react-router";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { keys } from "@/api/hooks";
import { apiError, LocationDisplay, mockApi, renderWithProviders, testUser } from "@/test/render";
import { makeRequest, makeRequestList, makeSummary } from "@/test/requests";
import { RequireAdmin, RequireAuth, useAuth } from "./auth";

beforeEach(() => {
  // No live updates in tests.
  vi.stubGlobal("EventSource", undefined);
});

function Greeting() {
  const user = useAuth();
  return <p>Hello {user.name}</p>;
}

function renderApp(route = "/library/2?sort=year") {
  return renderWithProviders(
    <Routes>
      <Route path="/login" element={<LocationDisplay />} />
      <Route path="/setup" element={<p>Setup</p>} />
      <Route element={<RequireAuth />}>
        <Route path="/library/:id" element={<Greeting />} />
        <Route path="/" element={<Greeting />} />
      </Route>
    </Routes>,
    { route },
  );
}

const server = { name: "pley", version: "1.0.0", setup_required: false };

describe("RequireAuth", () => {
  it("shows the page to a signed-in user", async () => {
    mockApi({ "GET /api/server": () => server, "GET /api/auth/me": () => ({ ...testUser, name: "Sam" }) });
    renderApp();
    expect(await screen.findByText("Hello Sam")).toBeInTheDocument();
  });

  it("sends everyone else to sign in, and back afterwards", async () => {
    mockApi({ "GET /api/server": () => server, "GET /api/auth/me": () => apiError(401, "unauthorized", "not signed in") });
    renderApp();
    expect(await screen.findByTestId("location")).toHaveTextContent("/login?next=%2Flibrary%2F2%3Fsort%3Dyear");
  });

  it("doesn't bother with the home page as a destination", async () => {
    mockApi({ "GET /api/server": () => server, "GET /api/auth/me": () => apiError(401, "unauthorized", "not signed in") });
    renderApp("/");
    expect((await screen.findByTestId("location")).textContent).toBe("/login");
  });

  it("sends a new server to setup", async () => {
    mockApi({ "GET /api/server": () => ({ ...server, setup_required: true }) });
    renderApp();
    expect(await screen.findByText("Setup")).toBeInTheDocument();
  });

  it("offers a retry when the server is unreachable", async () => {
    let up = false;
    mockApi({
      "GET /api/server": () => (up ? server : apiError(503, "unavailable", "starting up")),
      "GET /api/auth/me": () => testUser,
    });
    renderApp();

    expect(await screen.findByText("Can't reach the Pley server")).toBeInTheDocument();
    up = true;
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Hello Admin")).toBeInTheDocument();
  });

  it("explains other failures", async () => {
    mockApi({ "GET /api/server": () => server, "GET /api/auth/me": () => apiError(500, "internal", "database is locked") });
    renderApp();
    expect(await screen.findByText("Database is locked")).toBeInTheDocument();
  });

  it("listens afresh for another profile: an event stream and request notices of its own", async () => {
    const streams: { url: string; closed: boolean }[] = [];
    vi.stubGlobal(
      "EventSource",
      class {
        static readonly OPEN = 1;
        static readonly CLOSED = 2;
        readonly stream = { url: "", closed: false };
        constructor(url: string) {
          this.stream.url = url;
          streams.push(this.stream);
        }
        close() {
          this.stream.closed = true;
        }
      },
    );
    let me = { ...testUser, name: "Sam" };
    const news = makeRequest({ id: 4, title: "Heat", status: "available", unseen: true, updated_at: 1_700_000_500 });
    const requests = mockApi({
      "GET /api/server": () => server,
      "GET /api/auth/me": () => me,
      "GET /api/requests/summary": () => makeSummary({ unseen: 1, own: 1 }),
      "GET /api/requests": () => makeRequestList([news]),
    });
    const { client } = renderApp("/");
    expect(await screen.findByText("“Heat” is ready to watch.")).toBeInTheDocument();
    const catchUps = () => requests.filter((r) => r.path === "/api/requests?unseen=1&limit=20");
    expect(catchUps()).toHaveLength(1);

    me = { ...testUser, id: 2, name: "Ann", is_admin: false };
    act(() => {
      client.setQueryData(keys.me, me);
    });
    expect(await screen.findByText("Hello Ann")).toBeInTheDocument();
    expect(streams.map((s) => s.closed)).toEqual([true, false]);
    // Ann's news is hers to hear, however many Sam had.
    await waitFor(() => expect(catchUps()).toHaveLength(2));
  });
});

describe("RequireAdmin", () => {
  it("keeps settings to admins", () => {
    renderWithProviders(<RequireAdmin>Server settings</RequireAdmin>, { user: { ...testUser, is_admin: false } });
    expect(screen.getByText("Admins only")).toBeInTheDocument();
    expect(screen.queryByText("Server settings")).not.toBeInTheDocument();
  });

  it("shows them to admins", () => {
    renderWithProviders(<RequireAdmin>Server settings</RequireAdmin>);
    expect(screen.getByText("Server settings")).toBeInTheDocument();
  });
});

describe("useAuth", () => {
  it("must be used under RequireAuth", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Greeting />)).toThrow(/inside <RequireAuth>/);
  });
});
