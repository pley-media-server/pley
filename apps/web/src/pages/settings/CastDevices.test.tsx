import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { mockApi, renderWithProviders } from "@/test/render";
import { CastDevices } from "./CastDevices";

describe("CastDevices", () => {
  it("shows the server's choice, off by default, and saves a change at once", async () => {
    const user = userEvent.setup();
    let on = false;
    const requests = mockApi({
      "GET /api/settings": () => ({ cast_devices: on }),
      "PUT /api/settings": (req) => {
        on = (req.body as { cast_devices: boolean }).cast_devices;
        return { cast_devices: on };
      },
    });
    renderWithProviders(<CastDevices />);
    const toggle = await screen.findByRole("switch", { name: "Cast to Google Cast and AirPlay devices" });
    expect(toggle).not.toBeChecked();
    await user.click(toggle);
    await waitFor(() => expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ cast_devices: true }));
    await waitFor(() => expect(toggle).toBeChecked());
  });

  it("shows nothing for a server from before the setting", async () => {
    mockApi({ "GET /api/settings": () => ({ server_name: "Den" }) });
    const { container } = renderWithProviders(<CastDevices />);
    await waitFor(() => expect(container).toBeEmptyDOMElement());
  });
});
