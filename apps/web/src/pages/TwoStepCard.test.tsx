import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { TwoStepStatus } from "@/api/types";
import { apiError, mockApi, renderWithProviders, testUser } from "@/test/render";
import { TwoStepCard } from "./TwoStepCard";

const CODES = Array.from({ length: 10 }, (_, i) => `${i}aaa-bbbb-cccc-dddd`);

function serve(initial: TwoStepStatus = { enabled: false, since: null, recovery_codes_left: 0 }, { required = false } = {}) {
  let status = initial;
  return mockApi({
    "GET /api/server": () => ({ name: "Living Room", version: "1.0.0", setup_required: false, two_step_required: required }),
    "GET /api/me/two-step": () => status,
    "POST /api/me/two-step/setup": (req) => {
      const { password, code } = req.body as { password: string; code?: string };
      if (password !== "hunter22") return apiError(403, "forbidden", "the password is wrong");
      // On already: moving it to another app takes a code of the current one.
      if (status.enabled && code !== "654321") return apiError(401, "two_step", "that code didn't work");
      return { secret: "JBSWY3DPEHPK3PXP", uri: "otpauth://totp/pley:admin?secret=JBSWY3DPEHPK3PXP", qr: { size: 21, path: "M0 0h7v1h-7z" }, expires_at: 0 };
    },
    "POST /api/me/two-step": (req) => {
      if ((req.body as { code: string }).code !== "123456") return apiError(400, "bad_request", "that code didn't work");
      status = { enabled: true, since: 1_700_000_000, recovery_codes_left: 10 };
      return { recovery_codes: CODES };
    },
    "DELETE /api/me/two-step": (req) => {
      const { password, code } = req.body as { password: string; code: string };
      if (password !== "hunter22") return apiError(403, "forbidden", "the password is wrong");
      if (code !== "654321") return apiError(401, "two_step", "that code didn't work");
      status = { enabled: false, since: null, recovery_codes_left: 0 };
      return undefined;
    },
    "POST /api/me/two-step/recovery-codes": () => ({ recovery_codes: CODES }),
  });
}

describe("TwoStepCard", () => {
  it("sets up with the password, the QR code and a first code, then shows the recovery codes once", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderWithProviders(<TwoStepCard />);

    await user.click(await screen.findByRole("button", { name: "Set up" }));
    await user.type(screen.getByLabelText("Your password"), "wrong");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("The password is wrong")).toBeInTheDocument();
    await user.clear(screen.getByLabelText("Your password"));
    await user.type(screen.getByLabelText("Your password"), "hunter22");
    await user.click(screen.getByRole("button", { name: "Continue" }));

    expect(await screen.findByRole("img", { name: /QR code/ })).toBeInTheDocument();
    expect(screen.getByText("JBSW Y3DP EHPK 3PXP")).toBeInTheDocument();
    const code = screen.getByLabelText("Code from the app");
    await user.type(code, "999999");
    expect(await screen.findByText("That code didn't work")).toBeInTheDocument();
    await user.clear(code);
    await user.type(code, "123456");

    const list = await screen.findByRole("list", { name: "Recovery codes" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(10);
    expect(screen.getByText(/Devices already signed in stay signed in/)).toBeInTheDocument();
    expect(requests.filter((r) => r.method === "POST" && r.path === "/api/me/two-step").at(-1)?.body).toEqual({ code: "123456" });

    await user.click(screen.getByRole("button", { name: "I've saved them" }));
    expect(await screen.findByText(/On since/)).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Recovery codes" })).not.toBeInTheDocument();
    expect(screen.getByText("10 recovery codes left, for when your phone isn't at hand.")).toBeInTheDocument();
  });

  it("turns off with the password and a code", async () => {
    const user = userEvent.setup();
    serve({ enabled: true, since: 1_700_000_000, recovery_codes_left: 10 });
    renderWithProviders(<TwoStepCard />, { user: { ...testUser, two_step: true } });

    await user.click(await screen.findByRole("button", { name: "Turn off" }));
    const turnOff = screen.getByRole("button", { name: "Turn off" });
    await user.type(screen.getByLabelText("Your password"), "hunter22");
    expect(turnOff).toBeDisabled();
    await user.type(screen.getByLabelText("Code"), "111111");
    await user.click(turnOff);
    expect(await screen.findByText("That code didn't work")).toBeInTheDocument();
    await user.clear(screen.getByLabelText("Code"));
    await user.type(screen.getByLabelText("Code"), "654321");
    await user.click(turnOff);
    expect(await screen.findByRole("button", { name: "Set up" })).toBeInTheDocument();
  });

  it("makes new recovery codes with the password", async () => {
    const user = userEvent.setup();
    serve({ enabled: true, since: 1_700_000_000, recovery_codes_left: 2 });
    renderWithProviders(<TwoStepCard />, { user: { ...testUser, two_step: true } });

    expect(await screen.findByText("Only 2 recovery codes left: make new ones before they run out.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "New recovery codes" }));
    await user.type(screen.getByLabelText("Your password"), "hunter22");
    await user.click(screen.getByRole("button", { name: "Make new codes" }));
    expect(await screen.findByRole("list", { name: "Recovery codes" })).toBeInTheDocument();
    expect(screen.queryByText(/Devices already signed in/)).not.toBeInTheDocument();
  });

  it("moves to another app with the password and a code from the current one", async () => {
    const user = userEvent.setup();
    const requests = serve({ enabled: true, since: 1_700_000_000, recovery_codes_left: 4 });
    renderWithProviders(<TwoStepCard />, { user: { ...testUser, two_step: true } });

    await user.click(await screen.findByRole("button", { name: "Use another app" }));
    expect(screen.getByText(/the current one keeps working/)).toBeInTheDocument();
    const next = screen.getByRole("button", { name: "Continue" });
    await user.type(screen.getByLabelText("Your password"), "hunter22");
    expect(next).toBeDisabled();
    await user.type(screen.getByLabelText("Code from your current app"), "111111");
    await user.click(next);
    expect(await screen.findByText("That code didn't work")).toBeInTheDocument();
    await user.clear(screen.getByLabelText("Code from your current app"));
    await user.type(screen.getByLabelText("Code from your current app"), "654321");
    await user.click(next);

    await user.type(await screen.findByLabelText("Code from the app"), "123456");
    expect(await screen.findByText("Two-step sign-in uses your new app")).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Recovery codes" })).toBeInTheDocument();
    expect(screen.getByText(/old recovery codes, don't work any more/)).toBeInTheDocument();
    expect(requests.filter((r) => r.path === "/api/me/two-step/setup").at(-1)?.body).toEqual({ password: "hunter22", code: "654321" });
  });

  it("stays on while the server requires it", async () => {
    serve({ enabled: true, since: 1_700_000_000, recovery_codes_left: 10 }, { required: true });
    renderWithProviders(<TwoStepCard />, { user: { ...testUser, two_step: true } });

    expect(await screen.findByText(/This server requires it, so it stays on\./)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Turn off" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use another app" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "New recovery codes" })).toBeInTheDocument();
  });

  it("isn't there for accounts without a password", async () => {
    const requests = serve();
    renderWithProviders(<TwoStepCard />, { user: { ...testUser, has_password: false } });
    await waitFor(() => expect(requests).toHaveLength(0));
    expect(screen.queryByText("Two-step sign-in")).not.toBeInTheDocument();
  });
});
