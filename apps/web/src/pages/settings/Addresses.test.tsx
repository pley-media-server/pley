import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { mockApi, renderWithProviders } from "@/test/render";
import { Addresses, thisComputer } from "./Addresses";

const ADD_HERE = "Add the address you're using";

describe("thisComputer", () => {
  it("is an address no other device reaches", () => {
    for (const own of ["http://localhost:7539", "http://127.0.0.1:7539", "http://[::1]:7539", "http://pley.localhost"]) {
      expect(thisComputer(own)).toBe(true);
    }
    for (const reached of ["http://192.168.1.20:7539", "https://pley.example.com", "nonsense"]) {
      expect(thisComputer(reached)).toBe(false);
    }
  });
});

describe("Addresses", () => {
  /** A server with `saved` set and one address found at home. */
  function server(saved: string[]) {
    const requests = mockApi({
      "GET /api/settings": () => ({ addresses: saved }),
      "PUT /api/settings": (req) => {
        saved = (req.body as { addresses: string[] }).addresses;
        return { addresses: saved };
      },
      "GET /api/server/addresses": () => ({
        id: "0123456789abcdef0123456789abcdef01234567",
        addresses: [...saved.map((url) => ({ url, source: "set" })), { url: "http://10.0.0.4:7539", source: "found" }],
      }),
    });
    const saves = () => requests.filter((r) => r.method === "PUT").map((r) => r.body);
    return { requests, saves };
  }

  it("lists the administrator's addresses, then those found, and adds the page's own", async () => {
    const user = userEvent.setup();
    const here = "https://pley.example.com";
    vi.stubGlobal("location", { ...window.location, origin: here });
    const { saves } = server(["http://192.168.1.20:7539"]);
    renderWithProviders(<Addresses />);
    expect(await screen.findByText("Apps switch between these by themselves. Home addresses are added automatically.")).toBeInTheDocument();
    expect(await screen.findByText("http://10.0.0.4:7539")).toBeInTheDocument();
    expect(screen.getByText("Automatic")).toBeInTheDocument();
    // Found ones go as the server's networks do, not from here.
    expect(screen.getByRole("button", { name: "Remove http://192.168.1.20:7539" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remove http://10.0.0.4:7539" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: ADD_HERE }));
    await waitFor(() => expect(saves()).toEqual([{ addresses: ["http://192.168.1.20:7539", here] }]));
    expect(await screen.findByText(here)).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("button", { name: ADD_HERE })).not.toBeInTheDocument());

    await user.type(screen.getByRole("textbox", { name: "Address" }), "https://media.example.com{Enter}");
    await waitFor(() => expect(saves().at(-1)).toEqual({ addresses: ["http://192.168.1.20:7539", here, "https://media.example.com"] }));
    await user.click(await screen.findByRole("button", { name: "Remove http://192.168.1.20:7539" }));
    await waitFor(() => expect(saves().at(-1)).toEqual({ addresses: [here, "https://media.example.com"] }));
  });

  it("doesn't offer this computer's own address, or one found already", async () => {
    // The tests' page is on localhost.
    server([]);
    const { unmount } = renderWithProviders(<Addresses />);
    expect(await screen.findByText("http://10.0.0.4:7539")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: ADD_HERE })).not.toBeInTheDocument();
    unmount();

    vi.stubGlobal("location", { ...window.location, origin: "http://10.0.0.4:7539" });
    server([]);
    renderWithProviders(<Addresses />);
    expect(await screen.findByText("http://10.0.0.4:7539")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: ADD_HERE })).not.toBeInTheDocument();
  });

  it("shows nothing for a server from before the setting", async () => {
    const requests = mockApi({ "GET /api/settings": () => ({ server_name: "Den" }) });
    const { container } = renderWithProviders(<Addresses />);
    await waitFor(() => expect(requests.some((r) => r.path === "/api/settings")).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(requests.some((r) => r.path === "/api/server/addresses")).toBe(false);
  });
});
