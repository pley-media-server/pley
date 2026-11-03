import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ListenChoice, NetworkSettings as Network } from "@/api/types";
import { apiError, mockApi, renderWithProviders } from "@/test/render";
import { movedPage, NetworkSettings, stillServes } from "./NetworkSettings";

const NETWORK: Network = {
  web: ["127.0.0.1:7539"],
  jellyfin: [],
  jellyfin_on_web: false,
  plex: [],
  plex_on_web: true,
  discovery: true,
  fixed: { web: false, jellyfin: false, plex: false, discovery: false },
  listeners: [{ kind: "web", address: "127.0.0.1:7539", error: null }],
  adapters: [
    {
      name: "Ethernet",
      description: "Intel(R) I211 Gigabit Network Connection",
      up: true,
      addresses: ["192.168.1.10"],
      network: { name: "Home", kind: "private" },
    },
    { name: "Wi-Fi", description: null, up: false, addresses: ["10.0.0.5"], network: null },
  ],
  config_file: "C:\\ProgramData\\pley\\pley.toml",
  windows: true,
};

describe("NetworkSettings", () => {
  it("listens on the networks chosen", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/network": () => NETWORK,
      "PUT /api/network": (req) => {
        const choice = req.body as ListenChoice;
        return { ...NETWORK, ...choice, listeners: choice.web.map((address) => ({ kind: "web", address, error: null })) };
      },
    });
    renderWithProviders(<NetworkSettings />);
    expect(await screen.findByRole("radio", { name: /Only this computer/ })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();

    await user.click(screen.getByRole("radio", { name: /networks chosen/ }));
    expect(screen.getByText("Choose a network, or Only this computer.")).toBeInTheDocument();
    expect(screen.getByText("Private network")).toBeInTheDocument();
    expect(screen.getByText("Not connected")).toBeInTheDocument();
    expect(screen.getByText(/Windows Firewall decides/)).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: /Ethernet/ }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(requests.find((r) => r.method === "PUT")?.body).toEqual({
        web: ["127.0.0.1:7539", "192.168.1.10:7539"],
        jellyfin: [],
        jellyfin_on_web: false,
        plex: [],
        plex_on_web: true,
        discovery: true,
      }),
    );
    expect(await screen.findByText("Pley listens there now")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.10:7539")).toBeInTheDocument();
  });

  it("turns the Jellyfin-compatible API on, on pley's port or its own", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/network": () => ({ ...NETWORK, web: ["0.0.0.0:7539"] }),
      "PUT /api/network": (req) => ({ ...NETWORK, ...(req.body as ListenChoice) }),
    });
    renderWithProviders(<NetworkSettings />);
    expect(await screen.findByRole("radio", { name: /Every network/ })).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByRole("switch", { name: "Jellyfin-compatible API" }));
    const where = within(screen.getByRole("radiogroup", { name: "Jellyfin-compatible API's port" }));
    expect(where.getByRole("radio", { name: "On Pley's port (7539)" })).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(requests.filter((r) => r.method === "PUT").at(-1)?.body).toEqual({
        web: ["0.0.0.0:7539"],
        jellyfin: [],
        jellyfin_on_web: true,
        plex: [],
        plex_on_web: true,
        discovery: true,
      }),
    );

    // A port of its own: Jellyfin's, or another typed in.
    await user.click(where.getByRole("radio", { name: "On a port of its own" }));
    const port = screen.getByRole("spinbutton", { name: "Jellyfin port" });
    expect(port).toHaveValue(8096);
    await user.clear(port);
    await user.type(port, "7539");
    expect(screen.getByText("Choose another port for the Jellyfin-compatible API, or Pley's port.")).toBeInTheDocument();
    await user.clear(port);
    await user.type(port, "8920");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(requests.filter((r) => r.method === "PUT").at(-1)?.body).toEqual({
        web: ["0.0.0.0:7539"],
        jellyfin: ["0.0.0.0:8920"],
        jellyfin_on_web: false,
        plex: [],
        plex_on_web: true,
        discovery: true,
      }),
    );

    // And off again.
    await user.click(screen.getByRole("switch", { name: "Jellyfin-compatible API" }));
    expect(screen.queryByRole("spinbutton", { name: "Jellyfin port" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(requests.filter((r) => r.method === "PUT").at(-1)?.body).toEqual({
        web: ["0.0.0.0:7539"],
        jellyfin: [],
        jellyfin_on_web: false,
        plex: [],
        plex_on_web: true,
        discovery: true,
      }),
    );
  });

  it("has the Plex-compatible API on pley's port, and moves it to its own or turns it off", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/network": () => ({ ...NETWORK, web: ["0.0.0.0:7539"] }),
      "PUT /api/network": (req) => ({ ...NETWORK, ...(req.body as ListenChoice) }),
    });
    renderWithProviders(<NetworkSettings />);
    expect(await screen.findByRole("switch", { name: "Plex-compatible API" })).toHaveAttribute("aria-checked", "true");
    const plex = within(screen.getByRole("radiogroup", { name: "Plex-compatible API's port" }));
    expect(plex.getByRole("radio", { name: "On Pley's port (7539)" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText("Plex-compatible API, on Pley's port")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();

    // Plex's port, saved with the Jellyfin-compatible API's choice as it is.
    await user.click(plex.getByRole("radio", { name: "On a port of its own" }));
    expect(screen.getByRole("spinbutton", { name: "Plex port" })).toHaveValue(32400);
    expect(screen.getByText("Plex's own is 32400.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(requests.filter((r) => r.method === "PUT").at(-1)?.body).toEqual({
        web: ["0.0.0.0:7539"],
        jellyfin: [],
        jellyfin_on_web: false,
        plex: ["0.0.0.0:32400"],
        plex_on_web: false,
        discovery: true,
      }),
    );

    // The two APIs can't share a port of their own.
    await user.click(screen.getByRole("switch", { name: "Jellyfin-compatible API" }));
    const jellyfin = within(screen.getByRole("radiogroup", { name: "Jellyfin-compatible API's port" }));
    await user.click(jellyfin.getByRole("radio", { name: "On a port of its own" }));
    const jellyfinPort = screen.getByRole("spinbutton", { name: "Jellyfin port" });
    await user.clear(jellyfinPort);
    await user.type(jellyfinPort, "32400");
    expect(screen.getByText("The Jellyfin-compatible and Plex-compatible APIs can't share a port.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    await user.clear(jellyfinPort);
    await user.type(jellyfinPort, "8096");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(requests.filter((r) => r.method === "PUT").at(-1)?.body).toEqual({
        web: ["0.0.0.0:7539"],
        jellyfin: ["0.0.0.0:8096"],
        jellyfin_on_web: false,
        plex: ["0.0.0.0:32400"],
        plex_on_web: false,
        discovery: true,
      }),
    );

    // Off, said as such rather than left out.
    await user.click(screen.getByRole("switch", { name: "Plex-compatible API" }));
    expect(screen.queryByRole("radiogroup", { name: "Plex-compatible API's port" })).not.toBeInTheDocument();
    expect(screen.getByText(/such as Kometa and Tautulli/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(requests.filter((r) => r.method === "PUT").at(-1)?.body).toEqual({
        web: ["0.0.0.0:7539"],
        jellyfin: ["0.0.0.0:8096"],
        jellyfin_on_web: false,
        plex: [],
        plex_on_web: false,
        discovery: true,
      }),
    );
  });

  it("leaves the Plex-compatible API alone when the command line sets it", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/network": () => ({
        ...NETWORK,
        plex: ["0.0.0.0:32400"],
        plex_on_web: false,
        fixed: { web: false, jellyfin: false, plex: true, discovery: false },
        listeners: [
          { kind: "web", address: "127.0.0.1:7539", error: null },
          { kind: "plex", address: "0.0.0.0:32400", error: "address in use" },
        ],
      }),
      "PUT /api/network": (req) => ({ ...NETWORK, ...(req.body as ListenChoice) }),
    });
    renderWithProviders(<NetworkSettings />);
    expect(await screen.findByRole("switch", { name: "Plex-compatible API" })).toBeDisabled();
    expect(screen.getByText("Set by --plex-bind or PLEY_PLEX_BIND, on the command line or in the environment.")).toBeInTheDocument();
    expect(screen.getByRole("spinbutton", { name: "Plex port" })).toBeDisabled();
    const listener = screen.getByText("0.0.0.0:32400").closest("li");
    expect(listener).toHaveTextContent("Plex-compatible API");
    expect(listener).toHaveTextContent("Can't listen: address in use");

    await user.click(screen.getByRole("switch", { name: "Jellyfin-compatible API" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(requests.find((r) => r.method === "PUT")?.body).toEqual({
        web: ["127.0.0.1:7539"],
        jellyfin: [],
        jellyfin_on_web: true,
        plex: ["0.0.0.0:32400"],
        plex_on_web: false,
        discovery: true,
      }),
    );
  });

  it("says why the server refuses a choice", async () => {
    const user = userEvent.setup();
    mockApi({
      // On two ports, from the command line: the page shows the first.
      "GET /api/network": () => ({
        ...NETWORK,
        jellyfin: ["0.0.0.0:8096", "0.0.0.0:32400"],
        fixed: { web: false, jellyfin: true, plex: false, discovery: false },
      }),
      "PUT /api/network": () =>
        apiError(400, "bad_request", "the Plex-compatible API can't share port 32400 with the Jellyfin-compatible API"),
    });
    renderWithProviders(<NetworkSettings />);
    const plex = within(await screen.findByRole("radiogroup", { name: "Plex-compatible API's port" }));
    await user.click(plex.getByRole("radio", { name: "On a port of its own" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(
      await screen.findByText("The Plex-compatible API can't share port 32400 with the Jellyfin-compatible API"),
    ).toBeInTheDocument();
    // The choice stays, to change.
    expect(screen.getByRole("spinbutton", { name: "Plex port" })).toHaveValue(32400);
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
  });

  it("leaves what the command line sets alone, and says which addresses wait", async () => {
    mockApi({
      "GET /api/network": () => ({
        ...NETWORK,
        web: ["0.0.0.0:7539"],
        fixed: { web: true, jellyfin: false, plex: false, discovery: false },
        listeners: [{ kind: "jellyfin", address: "0.0.0.0:8096", error: "address in use" }],
      }),
    });
    renderWithProviders(<NetworkSettings />);
    expect(await screen.findByRole("radio", { name: /Every network/ })).toBeDisabled();
    expect(screen.getByText("PLEY_BIND")).toBeInTheDocument();
    expect(screen.getByRole("spinbutton", { name: "Port" })).toBeDisabled();
    expect(screen.getByText("Can't listen: address in use")).toBeInTheDocument();
  });

  it("knows when this page would stop working, or move", () => {
    const page = "http://localhost:7539/settings/network";
    expect(stillServes(["127.0.0.1:7539"], ["127.0.0.1:8000"], page)).toBe(false);
    expect(movedPage(["127.0.0.1:7539"], ["127.0.0.1:8000"], page)).toBe("http://localhost:8000/settings/network");
    // From another device, when pley stops listening on its network.
    expect(stillServes(["0.0.0.0:7539"], ["127.0.0.1:7539"], "http://192.168.1.10:7539/")).toBe(false);
    expect(movedPage(["0.0.0.0:7539"], ["127.0.0.1:7539"], "http://192.168.1.10:7539/")).toBeNull();
    expect(stillServes(["0.0.0.0:7539"], ["127.0.0.1:7539", "192.168.1.10:7539"], "http://192.168.1.10:7539/")).toBe(true);
    // Through a reverse proxy, which pley can't see past.
    expect(stillServes(["127.0.0.1:7539"], ["127.0.0.1:8000"], "https://pley.example.com/")).toBeNull();
    expect(stillServes(["127.0.0.1:7539"], ["127.0.0.1:8000"], "https://localhost/")).toBeNull();
  });
});
