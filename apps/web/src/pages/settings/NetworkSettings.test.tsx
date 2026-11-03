import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ListenChoice, NetworkSettings as Network } from "@/api/types";
import { mockApi, renderWithProviders } from "@/test/render";
import { movedPage, NetworkSettings, stillServes } from "./NetworkSettings";

const NETWORK: Network = {
  web: ["127.0.0.1:7539"],
  jellyfin: [],
  jellyfin_on_web: false,
  discovery: true,
  fixed: { web: false, jellyfin: false, discovery: false },
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
    expect(screen.getByRole("radio", { name: "On Pley's port (7539)" })).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(requests.filter((r) => r.method === "PUT").at(-1)?.body).toEqual({
        web: ["0.0.0.0:7539"],
        jellyfin: [],
        jellyfin_on_web: true,
        discovery: true,
      }),
    );

    // A port of its own: Jellyfin's, or another typed in.
    await user.click(screen.getByRole("radio", { name: "On a port of its own" }));
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
        discovery: true,
      }),
    );
  });

  it("leaves what the command line sets alone, and says which addresses wait", async () => {
    mockApi({
      "GET /api/network": () => ({
        ...NETWORK,
        web: ["0.0.0.0:7539"],
        fixed: { web: true, jellyfin: false, discovery: false },
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
