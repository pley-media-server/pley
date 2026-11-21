import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { MyTailscale } from "@/api/types";
import { apiError, mockApi, renderWithProviders, testUser } from "@/test/render";
import { TailscaleCard } from "./TailscaleCard";

const HERE = { login: "alice@example.com", name: "Alice", username: null };

function serve(initial: MyTailscale) {
  let mine = initial;
  return mockApi({
    "GET /api/me/tailscale": () => mine,
    "PUT /api/me/tailscale": (req) => {
      const { password, code } = req.body as { password: string; code?: string };
      if (password !== "hunter22") return apiError(403, "forbidden", "the password is wrong");
      if (code !== undefined && code !== "123456") return apiError(401, "two_step", "that code didn't work");
      mine = { ...mine, linked: { login: HERE.login, linked_at: 1_700_000_000 }, here: { ...HERE, username: testUser.username } };
      return mine;
    },
    "DELETE /api/me/tailscale": () => {
      mine = { ...mine, linked: null, here: HERE };
      return undefined;
    },
  });
}

describe("TailscaleCard", () => {
  it("links this device's Tailscale user with the password", async () => {
    const user = userEvent.setup();
    const requests = serve({ on: true, linked: null, here: HERE });
    renderWithProviders(<TailscaleCard />, { user: { ...testUser, is_admin: false } });

    expect(await screen.findByText("This device is signed in to Tailscale as alice@example.com.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Link alice@example.com" }));
    await user.type(screen.getByLabelText("Your password"), "hunter22");
    await user.click(screen.getByRole("button", { name: "Link" }));

    expect(await screen.findByText(/^Linked to alice@example.com since/)).toBeInTheDocument();
    expect(screen.getByText("This device is one of them.")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ password: "hunter22" });

    await user.click(screen.getByRole("button", { name: "Unlink" }));
    expect(await screen.findByRole("button", { name: "Link alice@example.com" })).toBeInTheDocument();
  });

  it("asks an administrator to turn on two-step sign-in first, then for its code", async () => {
    const user = userEvent.setup();
    serve({ on: true, linked: null, here: HERE, server_name: "media.example.ts.net" });
    const view = renderWithProviders(<TailscaleCard />);

    expect(await screen.findByText(/turn it on above first/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Link alice@example.com" })).toBeDisabled();
    view.unmount();

    const requests = serve({ on: true, linked: null, here: HERE });
    renderWithProviders(<TailscaleCard />, { user: { ...testUser, two_step: true } });
    await user.click(await screen.findByRole("button", { name: "Link alice@example.com" }));
    await user.type(screen.getByLabelText("Your password"), "hunter22");
    await user.type(screen.getByLabelText("Code"), "123456");
    await user.click(screen.getByRole("button", { name: "Link" }));

    expect(await screen.findByText(/^Linked to alice@example.com since/)).toBeInTheDocument();
    expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ password: "hunter22", code: "123456" });
  });

  it("says where to open Pley to link it, and why Tailscale can't be asked", async () => {
    serve({ on: true, linked: null, here: null, server_name: "media.example.ts.net", problem: "Tailscale isn't connected" });
    renderWithProviders(<TailscaleCard />);

    expect(
      await screen.findByText("To link it, open Pley at media.example.ts.net on a device signed in to Tailscale."),
    ).toBeInTheDocument();
    expect(screen.getByText("Pley can't ask Tailscale: Tailscale isn't connected")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Link/ })).not.toBeInTheDocument();
  });

  it("offers no link for a Tailscale user linked to another account", async () => {
    serve({ on: true, linked: null, here: { ...HERE, elsewhere: true } });
    renderWithProviders(<TailscaleCard />, { user: { ...testUser, is_admin: false } });

    expect(
      await screen.findByText(
        "This device is signed in to Tailscale as alice@example.com, which is linked to another Pley account.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Link/ })).not.toBeInTheDocument();
  });

  it("stays hidden while the server has it off and nothing is linked", async () => {
    const requests = serve({ on: false, linked: null, here: null });
    renderWithProviders(<TailscaleCard />);

    await waitFor(() => expect(requests.some((r) => r.path === "/api/me/tailscale")).toBe(true));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByText("Tailscale")).not.toBeInTheDocument();
  });
});
