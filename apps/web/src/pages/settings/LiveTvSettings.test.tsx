import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { LiveSettings } from "@/api/livetv";
import { mockApi, renderWithProviders } from "@/test/render";
import { LiveTvSettings } from "./LiveTvSettings";

const SETTINGS: LiveSettings = {
  access: "everyone",
  age_limited: false,
  deinterlace: true,
  direct_play: true,
  max_height: null,
  timeshift_minutes: 5,
  refresh_hours: 24,
  guide_days: 7,
  hide_offline: true,
  allowed_users: [],
  denied_users: [],
};

/** Settings → Live TV on its Options tab, with what the server has saved. */
async function options(saved: LiveSettings) {
  localStorage.setItem("pley.settings.live.tab", JSON.stringify("options"));
  const requests = mockApi({
    "GET /api/live/sources": () => [],
    "GET /api/live/settings": () => saved,
    "PUT /api/live/settings": (req) => req.body,
    "GET /api/users": () => [],
  });
  renderWithProviders(<LiveTvSettings />);
  await screen.findByText("Deinterlace");
  return requests;
}

afterEach(() => localStorage.clear());

describe("LiveTvSettings, Direct Play", () => {
  it("is on as the server has it, and can be turned off", async () => {
    const user = userEvent.setup();
    const requests = await options(SETTINGS);
    const directPlay = screen.getByRole("switch", { name: "Play channels as the provider sends them" });
    expect(directPlay).toBeChecked();

    await user.click(directPlay);
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(requests.find((r) => r.method === "PUT")?.body).toMatchObject({ direct_play: false, deinterlace: true }));
  });

  it("isn't offered by an older server, which has no such setting", async () => {
    const older = { ...SETTINGS };
    delete older.direct_play;
    await options(older);
    expect(screen.queryByRole("switch", { name: "Play channels as the provider sends them" })).not.toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Deinterlace interlaced channels" })).toBeInTheDocument();
  });
});
